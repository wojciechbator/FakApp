# VIRYA n8n production boundaries

n8n is an integration runtime, not a second application database.

## Keep in n8n

- external provider calls and credentials (Gmail, Gemini, Discord, Google Drive/Calendar/Sheets, Meta, Spotify/public web probes)
- schedules and verified ingress wiring
- deterministic payload shaping close to provider APIs
- bounded delivery buffers whose only job is avoiding replay of an external side effect
- provider-specific health checks and alerts

## Keep in CrowdRelay

- durable business/correlation state
- idempotency and lifecycle state
- policy/scoring/selection
- capabilities, approval and autonomy rules
- provider-confirmed action outcomes and measurements
- reusable team/ecosystem contracts

## Current exceptions kept intentionally

`VOSRECEIPT00001` keeps a small technical delivery spool because it prevents replaying a provider side effect when CrowdRelay is temporarily unavailable. Error/watchdog static state is technical alert de-duplication, not business truth. `VIRYA 00 — Cockpit Rebuilder` still uses a one-day technical snapshot guard; migrate only when that cockpit is replaced, not in a blind rewrite.

## Active legacy candidates

Playlist Pitching, Social Ninja, META Collector/Hashtag Radar and the Sheets-backed cockpit remain active because they have real provider/operational behavior. Do not rewrite them merely to reduce node count. Any future change should first move durable state/decision policy to CrowdRelay, then leave a thin provider adapter here.

`VIRYA 11` is already reduced to entity recognition + official-domain citation. `VOSMAIL` is stateless with CrowdRelay-owned Gmail correlation. `VIRYA 23` relies on CrowdRelay idempotency rather than a local seen-release registry.
