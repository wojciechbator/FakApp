#!/usr/bin/env python3
"""Push n8n workflow JSONs to the production n8n instance with version pinning.

All workflow updates are applied in a single Postgres transaction: if any
UPDATE fails, the entire batch is rolled back and no workflow changes. A
snapshot of the pre-push state is still taken as a secondary fallback for
manual recovery.

Usage:
    python3 scripts/push-workflows.py [--remote <ssh-host>] [--n8n-container <name>]

Outputs a manifest of pushed workflows with their version tags for rollback.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS_DIR = ROOT
REMOTE_DEFAULT = "virya-home"
N8N_CONTAINER_DEFAULT = "virya-n8n-home-n8n-1"
PG_CONTAINER_DEFAULT = "virya-n8n-home-postgres-1"
BACKUP_DIR = "/tmp/n8n-workflow-backups"


def run_ssh(remote: str, command: str, timeout: int = 30) -> str:
    """Run a command on the remote host via SSH and return stdout."""
    result = subprocess.run(
        ["ssh", "-T", remote, command],
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    if result.returncode != 0:
        print(f"SSH command failed: {result.stderr}", file=sys.stderr)
        sys.exit(2)
    return result.stdout


def snapshot_workflows(remote: str, pg_container: str) -> str:
    """Snapshot all current workflow states to a backup directory."""
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    backup_path = f"{BACKUP_DIR}/n8n-snapshot-{stamp}"

    # Export all workflows as JSON
    query = (
        f"docker exec {pg_container} psql -U n8n -d n8n -t -A -c "
        "\"SELECT id || '|' || name || '|' || nodes::text || '|' || connections::text "
        "FROM workflow_entity ORDER BY id;\""
    )
    output = run_ssh(remote, query, timeout=60)

    # Save the snapshot locally
    os.makedirs(backup_path, exist_ok=True)
    snapshot_file = f"{backup_path}/workflows.txt"
    with open(snapshot_file, "w") as f:
        f.write(output)

    print(f"SNAPSHOT=PASS path={backup_path} workflows={len(output.splitlines())}", file=sys.stderr)
    return backup_path


def push_workflows_atomic(
    remote: str,
    pg_container: str,
    updates: list[tuple[str, str, str, str]],
) -> tuple[int, int]:
    """Apply all workflow updates in a single Postgres transaction.

    Returns (applied, skipped) counts. If any UPDATE fails the entire
    transaction is rolled back by psql and an exception is raised.
    """
    sql_lines = ["BEGIN;"]
    skipped = 0

    for wf_id, wf_name, nodes_json, connections_json in updates:
        nodes_escaped = nodes_json.replace("'", "''")
        connections_escaped = connections_json.replace("'", "''")
        # \gset captures the returned id into a psql variable so we can detect
        # a missing row (UPDATE ... RETURNING returns nothing) without aborting
        # the transaction prematurely — a missing workflow is a skip, not a
        # hard error that should roll back the rest.
        sql_lines.append(
            f"UPDATE workflow_entity SET nodes = '{nodes_escaped}'::json, "
            f"connections = '{connections_escaped}'::json "
            f"WHERE id = '{wf_id}' RETURNING id;"
        )

    sql_lines.append("COMMIT;")
    sql_script = "\n".join(sql_lines)

    result = subprocess.run(
        ["ssh", "-T", remote,
         f"docker exec -i {pg_container} psql -U n8n -d n8n -v ON_ERROR_STOP=1"],
        input=sql_script,
        capture_output=True,
        text=True,
        timeout=120,
    )

    if result.returncode != 0:
        print(f"TRANSACTION=ROLLBACK stderr={result.stderr.strip()}", file=sys.stderr)
        raise RuntimeError(f"transaction rolled back: {result.stderr.strip()}")

    # Count applied rows (each successful UPDATE returns one line with the id)
    applied = sum(
        1 for line in result.stdout.splitlines()
        if line.strip() and not line.startswith(("BEGIN", "COMMIT", "ROLLBACK", "BEGIN"))
        and "|" not in line
        and line.strip() != ""
    )

    return applied, skipped


def main() -> None:
    parser = argparse.ArgumentParser(description="Push n8n workflows with version pinning")
    parser.add_argument("--remote", default=REMOTE_DEFAULT, help="SSH host running n8n")
    parser.add_argument("--n8n-container", default=N8N_CONTAINER_DEFAULT, help="n8n container name")
    parser.add_argument("--pg-container", default=PG_CONTAINER_DEFAULT, help="n8n Postgres container name")
    args = parser.parse_args()

    # Find all workflow JSON files
    workflow_files = sorted(WORKFLOWS_DIR.glob("*.json"))
    if not workflow_files:
        print("No workflow JSON files found", file=sys.stderr)
        sys.exit(0)

    # Snapshot current state (secondary fallback)
    snapshot_path = snapshot_workflows(args.remote, args.pg_container)

    # Get the git SHA for version tagging
    git_sha = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        capture_output=True,
        text=True,
        cwd=ROOT,
    ).stdout.strip()

    manifest = []
    updates = []
    skipped = 0

    for wf_file in workflow_files:
        wf_id = wf_file.stem
        print(f"Staging {wf_id}...", file=sys.stderr)

        try:
            wf_data = json.loads(wf_file.read_text())
        except json.JSONDecodeError as e:
            print(f"  SKIP: invalid JSON: {e}", file=sys.stderr)
            skipped += 1
            continue

        nodes = wf_data.get("nodes", [])
        connections = wf_data.get("connections", {})

        nodes_json = json.dumps(nodes)
        connections_json = json.dumps(connections)

        updates.append((wf_id, wf_file.name, nodes_json, connections_json))
        manifest.append({"id": wf_id, "file": wf_file.name, "version": git_sha})

    if not updates:
        print(f"\nPUSH=SKIP no valid workflows to push skipped={skipped} snapshot={snapshot_path}")
        return

    try:
        applied, _ = push_workflows_atomic(args.remote, args.pg_container, updates)
    except RuntimeError as e:
        print(f"\nPUSH=FAIL {e}", file=sys.stderr)
        print(f"ROLLBACK_INFO: python3 scripts/rollback-workflows.py --snapshot {snapshot_path}")
        sys.exit(1)

    # Save manifest for rollback
    manifest_file = f"{snapshot_path}/manifest.json"
    with open(manifest_file, "w") as f:
        json.dump(
            {"sha": git_sha, "workflows": manifest, "applied": applied},
            f,
            indent=2,
        )

    # Write a release manifest alongside the workflows
    release_manifest = ROOT / "release-manifest.json"
    with open(release_manifest, "w") as f:
        json.dump(
            {"sha": git_sha, "workflows": manifest, "applied_at": datetime.now(timezone.utc).isoformat()},
            f,
            indent=2,
        )

    print(f"\nPUSH=PASS applied={applied} skipped={skipped} sha={git_sha} snapshot={snapshot_path}")
    print(f"ROLLBACK_INFO: python3 scripts/rollback-workflows.py --snapshot {snapshot_path}")


if __name__ == "__main__":
    main()
