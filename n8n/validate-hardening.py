#!/usr/bin/env python3
"""Offline integrity checks for the hardened VIRYA OS n8n export.

No credentials or network access are required. The validator checks graph
integrity, execution-data persistence, execution-receipt wiring, heartbeat
manifest drift and JavaScript syntax (when `node` is installed).
"""
from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
RECEIPT_ID = "VOSRECEIPT00001"
HEARTBEAT_ID = "VOSHEARTBEAT001"
TEAM_OPS_ID = "VOSTEAMOPS00001"
TEAM_EMAIL_ID = "VOSTEAMEMAIL001"
CALENDAR_ID = "VOSCAL000000001"
SENSITIVE_PREFIX = "CrowdRelay — VIRYA OS"
INGRESS_ID = "44808b183edf4ac7"
REQUIRED_RECEIPT_EXECUTORS = {
    "VOSMAIL000000001",
    "VOSAPP000000001",
    "VOSFUND00000001",
    "VOSFUND00000002",
    TEAM_OPS_ID,
    TEAM_EMAIL_ID,
    CALENDAR_ID,
}
CLAIMED_PROVIDER_EXECUTORS = REQUIRED_RECEIPT_EXECUTORS - {CALENDAR_ID}
OLD_PROGRESS_NODES = {
    "Confirm submitted to CrowdRelay",
    "Confirm package ready",
    "Confirm funding submitted",
}
PROVIDER_SUCCESS_EDGES = {
    "VOSMAIL000000001": "Gmail — VIRYA OS SEND",
    "VOSAPP000000001": "Gmail — submit opportunity",
    "VOSFUND00000001": "Drive — upload funding package",
    "VOSFUND00000002": "Gmail — submit funding",
    TEAM_OPS_ID: "Discord — execute team action",
    TEAM_EMAIL_ID: "Gmail — send team task",
}


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def node_names(workflow: dict) -> set[str]:
    return {str(node.get("name", "")) for node in workflow.get("nodes", [])}


def code_text(workflow: dict) -> str:
    return "\n".join(
        str(node.get("parameters", {}).get("jsCode", ""))
        for node in workflow.get("nodes", [])
        if node.get("type") == "n8n-nodes-base.code"
    )


def has_execute_workflow_target(workflow: dict, target: str) -> bool:
    for node in workflow.get("nodes", []):
        if node.get("type") != "n8n-nodes-base.executeWorkflow":
            continue
        value = node.get("parameters", {}).get("workflowId", {})
        if isinstance(value, dict):
            value = value.get("value")
        if value == target:
            return True
    return False


def main() -> int:
    errors: list[str] = []
    paths = sorted(ROOT.glob("*.json"))
    workflows: dict[str, dict] = {}
    metadata: dict[str, object] = {}

    for path in paths:
        try:
            document = json.loads(path.read_text(encoding="utf-8"))
        except Exception as exc:  # noqa: BLE001
            fail(errors, f"{path.name}: invalid JSON: {exc}")
            continue
        is_workflow = (
            isinstance(document, dict)
            and all(key in document for key in ("id", "name", "active", "nodes", "connections"))
            and isinstance(document.get("nodes"), list)
            and isinstance(document.get("connections"), dict)
        )
        if not is_workflow:
            metadata[path.name] = document
            continue
        workflow = document
        workflow_id = str(workflow.get("id") or path.stem)
        if workflow_id in workflows:
            fail(errors, f"duplicate workflow id: {workflow_id}")
        workflows[workflow_id] = workflow

        names = [str(node.get("name", "")) for node in workflow.get("nodes", [])]
        if len(names) != len(set(names)):
            fail(errors, f"{workflow_id}: duplicate node names")
        known = set(names)
        for source, output_groups in workflow.get("connections", {}).items():
            if source not in known:
                fail(errors, f"{workflow_id}: connection source missing: {source}")
            if not isinstance(output_groups, dict):
                continue
            for outputs in output_groups.values():
                for branch in outputs or []:
                    for edge in branch or []:
                        if not isinstance(edge, dict):
                            fail(errors, f"{workflow_id}: malformed connection edge: {edge!r}")
                            continue
                        target = edge.get("node")
                        if target not in known:
                            fail(errors, f"{workflow_id}: connection target missing: {target}")

        name = str(workflow.get("name", ""))
        if name.startswith(SENSITIVE_PREFIX) or workflow_id == INGRESS_ID:
            settings = workflow.get("settings", {})
            expected = {
                "saveDataErrorExecution": "none",
                "saveDataSuccessExecution": "none",
                "saveManualExecutions": False,
                "saveExecutionProgress": False,
            }
            for key, value in expected.items():
                if settings.get(key) != value:
                    fail(errors, f"{workflow_id}: unsafe {key}={settings.get(key)!r}")

    if len(workflows) != 57:
        fail(errors, f"expected exactly 57 release workflow definitions, found {len(workflows)}")

    expected_metadata = {"BRIDGE_ROUTE_DELTA.json"}
    unexpected_metadata = set(metadata) - expected_metadata
    if unexpected_metadata:
        fail(errors, f"unexpected root JSON metadata files: {sorted(unexpected_metadata)}")
    bridge_delta = metadata.get("BRIDGE_ROUTE_DELTA.json")
    expected_bridge_delta = {"viryaos.team.assignment_email_requested": TEAM_EMAIL_ID}
    if bridge_delta != expected_bridge_delta:
        fail(errors, f"BRIDGE_ROUTE_DELTA.json drifted: expected {expected_bridge_delta!r}, got {bridge_delta!r}")

    for required in [RECEIPT_ID, HEARTBEAT_ID, TEAM_OPS_ID, TEAM_EMAIL_ID, CALENDAR_ID, INGRESS_ID]:
        if required not in workflows:
            fail(errors, f"missing required workflow {required}")

    inactive = {workflow_id: str(workflow.get("name", "")) for workflow_id, workflow in workflows.items() if not workflow.get("active")}
    if set(inactive) != {TEAM_EMAIL_ID}:
        fail(errors, f"release reference must mirror 56 active runtime workflows plus only the new manual-activation team email executor; inactive={sorted(inactive)}")
    if len(workflows) != 57 or sum(bool(w.get("active")) for w in workflows.values()) != 56:
        fail(errors, f"release workflow count drift total={len(workflows)} active={sum(bool(w.get('active')) for w in workflows.values())}")
    team_email = workflows.get(TEAM_EMAIL_ID, {})
    if team_email.get("name") != "VIRYA — Team Task Email Executor":
        fail(errors, "team email workflow must keep its exact UI-discoverable name")

    seo = workflows.get("KUlmtYcXDIbfBYEm", {})
    if seo:
        seo_nodes = seo.get("nodes", [])
        if len(seo_nodes) > 12:
            fail(errors, "lean entity-visibility workflow regressed above 12 nodes")
        if any("googleSheets" in str(node.get("type", "")) for node in seo_nodes):
            fail(errors, "entity-visibility workflow must not use Google Sheets as state")
        if "Visibility_Score" in code_text(seo) or "SEO_Score" in code_text(seo):
            fail(errors, "entity-visibility workflow must not fabricate synthetic SEO scores")

    mail = workflows.get("VOSMAIL000000001", {})
    mail_code = code_text(mail)
    for forbidden in ("$getWorkflowStaticData", "pending_outreach", "store.threads", "last_reply_id"):
        if forbidden in mail_code:
            fail(errors, f"mail executor keeps durable business state in n8n: {forbidden}")
    if "/internal/autopilot/provider-actions/" not in mail_code and "/internal/autopilot/provider-actions/" not in json.dumps(mail, ensure_ascii=False):
        fail(errors, "mail reply monitor must resolve provider correlation from CrowdRelay")

    receipt = workflows.get(RECEIPT_ID, {})
    receipt_serialized = json.dumps(receipt, ensure_ascii=False)
    if not receipt.get("active"):
        fail(errors, "receipt spooler must be active")
    for token in ["/execution-report", "pending_receipts", "receipt_key", "Every 5m"]:
        if token not in receipt_serialized:
            fail(errors, f"receipt spooler missing contract token {token!r}")
    if "continueErrorOutput" not in receipt_serialized:
        fail(errors, "receipt delivery must preserve queue on HTTP failure")

    heartbeat = workflows.get(HEARTBEAT_ID, {})
    heartbeat_serialized = json.dumps(heartbeat, ensure_ascii=False)
    if not heartbeat.get("active"):
        fail(errors, "heartbeat workflow must be active")
    for token in ["/executors/heartbeat", "VIRYAOS_ENABLE_TEAM_OPS", "VIRYAOS_ENABLE_FUNDING_PACKAGE", "VIRYAOS_ENABLE_CALENDAR", "VIRYAOS_ENABLE_TEAM_EMAIL"]:
        if token not in heartbeat_serialized:
            fail(errors, f"heartbeat missing contract token {token!r}")
    # promotion.budget must not occur in executable heartbeat code; it is intentionally fail-closed.
    if "promotion.budget" in code_text(heartbeat):
        fail(errors, "heartbeat must not advertise promotion.budget without a provider adapter")

    manifest = ROOT / "workflow-manifest.tsv"
    if not manifest.exists():
        fail(errors, "workflow-manifest.tsv missing")
    else:
        digest = hashlib.sha256(manifest.read_bytes()).hexdigest()
        if digest not in code_text(heartbeat):
            fail(errors, f"heartbeat fallback manifest SHA drifted: {digest}")
        hardening = (ROOT / "VIRYAOS-HARDENING.md")
        if not hardening.exists() or digest not in hardening.read_text(encoding="utf-8"):
            fail(errors, "VIRYAOS-HARDENING.md manifest SHA drifted")

    for workflow_id in REQUIRED_RECEIPT_EXECUTORS:
        workflow = workflows.get(workflow_id)
        if workflow is None:
            fail(errors, f"missing receipt-aware executor {workflow_id}")
            continue
        serialized = json.dumps(workflow, ensure_ascii=False)
        if not has_execute_workflow_target(workflow, RECEIPT_ID):
            fail(errors, f"{workflow_id}: does not call receipt spooler")
        if "status:'succeeded'" not in code_text(workflow) and 'status: "succeeded"' not in code_text(workflow):
            fail(errors, f"{workflow_id}: no provider success receipt")
        if workflow_id in CLAIMED_PROVIDER_EXECUTORS:
            if "/execution-claim" not in serialized or "claim_token" not in code_text(workflow):
                fail(errors, f"{workflow_id}: non-idempotent provider call lacks pre-provider execution claim")
            if "Fail closed on ambiguous" not in serialized and "outcome ambiguous" not in code_text(workflow):
                fail(errors, f"{workflow_id}: ambiguous provider outcome is not fail-closed")
        provider_name = PROVIDER_SUCCESS_EDGES.get(workflow_id)
        provider_nodes = [node for node in workflow.get("nodes", []) if node.get("name") == provider_name] if provider_name else []
        if workflow_id in CLAIMED_PROVIDER_EXECUTORS:
            for provider in provider_nodes:
                if provider.get("retryOnFail"):
                    fail(errors, f"{workflow_id}: non-idempotent provider node must not retry after execution claim")
                if provider.get("onError") != "continueErrorOutput":
                    fail(errors, f"{workflow_id}: provider failure output is not wired explicitly")
        for provider in provider_nodes:
            if "discord" in str(provider.get("name", "")).lower():
                params = provider.get("parameters", {})
                if params.get("authentication") != "genericCredentialType" or params.get("genericAuthType") != "httpHeaderAuth":
                    fail(errors, f"{workflow_id}: Discord provider credential is present but HTTP auth is not enabled")
        if workflow_id in {"VOSAPP000000001", "VOSFUND00000001", "VOSFUND00000002"}:
            stale = node_names(workflow) & OLD_PROGRESS_NODES
            if stale:
                fail(errors, f"{workflow_id}: redundant progress callback remains: {sorted(stale)}")

    calendar = workflows.get(CALENDAR_ID, {})
    if not calendar.get("active"):
        fail(errors, "release reference should mirror the user's currently active Calendar workflow")
    if "Prepare Calendar success receipt" not in node_names(calendar):
        fail(errors, "Calendar provider completion receipt is missing")
    if "action_id" not in code_text(calendar):
        fail(errors, "Calendar receipt cannot correlate to the CrowdRelay action")
    bindings = ROOT / "RUNTIME-BINDINGS.md"
    if "REPLACE_WITH_GOOGLE_CALENDAR_CREDENTIAL" in json.dumps(calendar, ensure_ascii=False):
        if (
            not bindings.exists()
            or "preserve" not in bindings.read_text(encoding="utf-8").lower()
            or "Calendar" not in bindings.read_text(encoding="utf-8")
        ):
            fail(errors, "Calendar reference has a placeholder credential but no preserve-live-binding deployment contract")

    funding_package = workflows.get("VOSFUND00000001", {})
    if not funding_package.get("active"):
        fail(errors, "release reference should mirror the user's currently active funding package executor")

    node = shutil.which("node")
    checked = 0
    if node:
        with tempfile.TemporaryDirectory(prefix="virya-n8n-js-") as tmp:
            tmp_path = Path(tmp)
            for workflow_id, workflow in workflows.items():
                for index, n in enumerate(workflow.get("nodes", [])):
                    if n.get("type") != "n8n-nodes-base.code":
                        continue
                    js = str(n.get("parameters", {}).get("jsCode", ""))
                    # n8n Code nodes allow top-level return; wrap them in a function for parser validation.
                    probe = tmp_path / f"{workflow_id}-{index}.js"
                    probe.write_text("async function __n8n_code__(){\n" + js + "\n}\n", encoding="utf-8")
                    checked += 1
                    try:
                        result = subprocess.run(
                            [node, "--check", str(probe)],
                            capture_output=True,
                            text=True,
                            timeout=10,
                        )
                    except subprocess.TimeoutExpired:
                        fail(errors, f"{workflow_id}/{n.get('name')}: JavaScript syntax check timed out after 10s")
                        continue
                    if result.returncode:
                        fail(errors, f"{workflow_id}/{n.get('name')}: JavaScript syntax error: {result.stderr.strip()}")

    if errors:
        print(f"VIRYAOS_N8N_HARDENING=FAIL workflows={len(workflows)} code_nodes={checked}")
        for item in errors:
            print(f"- {item}")
        return 1

    active_count = sum(bool(workflow.get("active")) for workflow in workflows.values())
    print(
        f"VIRYAOS_N8N_HARDENING=PASS workflows={len(workflows)} active={active_count} "
        f"gated={len(workflows) - active_count} metadata={len(metadata)} code_nodes={checked} "
        "graph=valid receipts=closed-loop heartbeat=fail-closed bridge-delta=valid"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
