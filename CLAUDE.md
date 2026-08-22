# CLAUDE.md — dev workspace root

Cross-repo policy + index only. Per-repo facts live in each repo's own CLAUDE.md
(loaded automatically when files in that repo are touched). Do not duplicate them here.

## MODE
- **FIRST:** enter `/caveman`. Stay in caveman for the whole task.
- Think: inspect → smallest fix → verify. Short output. No tutorials. No fluff.
- Do not ask for facts you can inspect locally or on GitHub.
- No speculative refactors, broad cleanup, or dependency churn unless asked.

## REPO INDEX
All repos live under `/Users/wojciechbator/dev/<name>`, all on `main`, all separate git repos.

| Repo | Role | Stack | Canonical gate |
|---|---|---|---|
| `crowdrelay` | durable business/backend authority | Rust 1.97.1, Axum 0.8, SQLx 0.8, Postgres 18 | `make ci` (fast: `make check`) |
| `crowdrelay-control-plane` | infra/operator plane, never tenant-critical | Rust, Axum 0.8, SQLx, Postgres + SolidJS | `make ci` (fast: `make static`) |
| `virya` | production virya.music site/commerce/tickets/AREA/staff | Astro 7 SSR on Netlify + Preact 10 + Tailwind 4 | `npm run quality` |
| `virya-signal` | Tauri 2 + Leptos 0.8 mobile client, not a backend | Rust 1.97, wasm32 + Android | see repo file (multi-target) |
| `synesthesia` | Godot 4.7 + Rust experience layer | GDScript + godot-rust 0.5.5 | `./validate.sh` (fast: `./scripts/validate-fast.sh`) |
| `bator-blog` | separate product, unrelated architecture | Astro + Preact on Netlify | `npm run quality` |

Not in the map, no CLAUDE.md, treat as unrelated unless asked: `leaderguard`, `ledgerguard` (Rust),
`voter` (Node), `n8n` (not a git repo), `agent`, `.agents`.

n8n / Drive / Sheets / Calendar / email / Stripe / InPost / Bandsintown / Meta are
adapters and execution surfaces, never durable authority.

## TRUTH
- **Live repo + current `origin/main` + open PR/CI + runtime evidence > chat memory.**
- Before meaningful work: `git status --short --branch`; inspect HEAD/main/PR/CI as needed.
- Read only relevant files. Prefer `rg`, `git grep`, focused diffs/logs.
- Historical context is for traps and preferences, not source of truth.

## CROSS-REPO RULE
- Business policy belongs in CrowdRelay domain code, not in clients or workflows.
- OpenAPI and contract tests are the compatibility boundary.
- Contract/auth/event/data-shape change: search consumers before merge.
  CrowdRelay consumers are `virya/src/server` + `virya/src/lib/crowdrelay*.ts`,
  `virya-signal/src-tauri/src/api`, and `synesthesia` reward/run contract.
- Do not hide a backend contract bug in callers.

## SHARED CONVENTIONS
Every repo here follows the same pattern; assume it before inspecting:
- Contract/policy gates are Python `scripts/test_*.py` or `scripts/check-*.py` plus Node `scripts/audit-*.mjs`.
- `scripts/check-ci-policy.py` exists in most repos and asserts the CI workflow matches the local gate.
- Ratchet files (`scripts/*-ratchet.json`) are baselines that may shrink freely and must not grow.
- `VERSION`, `SECURITY.md`, `RELEASE.md` at repo root where present are real, maintained files.

## DEBUG / CHANGE LOOP
Before editing: 1) broken invariant? 2) owning layer? 3) smallest safe fix? 4) proof plan?
After editing: `git diff --check`; inspect diff/stat/status; run the narrowest relevant test first,
then the repo's canonical gate before claiming ready.

CI: find the **first real failure**; classify code/contract/dependency/environment/flaky.
Fix root cause. Do not blindly rerun everything.

Deployment: `changed != tested != pushed != merged != deployed != runtime-verified`.
Exact SHA + runtime health/revision matter.

## GIT SAFETY
Never silently: `reset --hard` over user work; `git clean -fd`; force-push `main`;
rewrite useful public history; disable tests or security gates to get green.
Non-fast-forward: fetch → inspect divergence → preserve work → rebase/merge → validate → push.

## AUTOPILOT
Rust/CrowdRelay is authority; n8n is an execution adapter.
Levels: `AUTO` / `BOUNDED AUTO` / `APPROVAL`. Paid, contractual, or irreversible actions stay approval-gated.
Live-show annual cap: operator config → validated ingestion → persisted CrowdRelay value → policy reads last valid value.

## SECRETS
Never print or commit production secrets, bearer tokens, private keys, `.env`,
ticket capabilities, or staff/admin credentials. Inspect shape and existence, not values.

## COMMITS
Conventional Commits. No AI attribution, no `Co-Authored-By: Claude`, no generated-with footers.

## OUTPUT
Report only what changed, what was verified, and the exact SHA/PR/run when useful.
State uncertainty instead of guessing.

## OPS HOSTS (verified 2026-08-22, after the consolidation onto virya-crowdrelay)
Three SSH aliases, keys already configured; probe before assuming state.

`ssh virya-crowdrelay` — **production**. Oracle A1.Flex, Ubuntu 24.04, aarch64,
`ubuntu@152.70.162.119` (reserved IP), 2 OCPU / 12 GB RAM, 45G disk. Runs (docker):
`crowdrelay-api-1`, `crowdrelay-worker-1`, `crowdrelay-rekor-proof-anchor`,
`crowdrelay-area-management-proxy-1`, `virya-postgres18`,
`crowdrelay-control-plane-{app,postgres,virya-area-tunnel}-1`, `virya-edge-caddy`.
Serves control.virya.music, signal-api.virya.music and n8n.virya.music.
Images must be **arm64**; an amd64-only tag cannot be pulled here.
Host state that no repo holds is in `crowdrelay/ops/HOST_BOOTSTRAP.md` — read it before
rebuilding this box or debugging a firewall, Postgres role or edge auth problem.

`ssh virya-home` — Debian 13, `wojtek@192.168.100.27` (LAN only), 16 GB RAM, 30G root.
Runs `virya-n8n-home-{n8n,postgres}-1` and the `immich_*` stack (photos, unrelated to Virya).
n8n binds `10.77.0.2:5678`, its WireGuard address, and is published through the production
edge; `oracle-bridge` is retired behind a compose profile. The stopped
`crowdrelay-control-plane-*` containers and their volume are the migration rollback.
Locale is Polish — `free`/`df` headers come back in Polish, do not parse them by header name.

`ssh virya-oracle` — Oracle free tier, Ubuntu 24.04, `ubuntu@141.144.230.39`, 45G disk.
**Only 954 MB RAM.** No longer production: it runs `virya-postgres18` alone, holding the
pre-migration data as rollback, with all CrowdRelay timers disabled. Earmarked for a small
FakApp watchdog. Do not point anything at it.

WireGuard `wg0` (production `10.77.0.1` to virya-home `10.77.0.2`) is the only cross-host
link. Tailscale carries nothing and is not installed on the production host.

Ops rules: read-only probes first (`docker ps`, `docker logs --tail`, `systemctl status`);
never restart or redeploy production without saying so first; deploys go through each repo's
canonical deploy script, not by hand on the host.

## WORKING AGREEMENT (token discipline)
The account is Claude Pro, so context is the scarce resource. Per turn:
- Batch independent shell work into one call; prefer `rg -n`/`sed -n 'A,Bp'` over reading whole files.
- Never re-derive what this file or a repo CLAUDE.md already states.
- Do not spawn subagents unless asked — a subagent re-reads context from cold.
- Final answer = what changed, what was verified, what is left. No plan preamble, no recap of the diff.
