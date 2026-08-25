# VIRYA OS n8n hardening

Manifest SHA: `9a4e58807417fc3504bbecd9c2b9d983fdcce7bbd26798e120229f9223df6ff1`

- `VOSRECEIPT00001` queues execution receipts before delivery and retries every 5 minutes. Provider success is never replayed merely because the receipt API is temporarily unavailable.
- `VOSHEARTBEAT001` refreshes the capability registry every 30 minutes with a 90-minute TTL.
- `VOSTEAMOPS00001` executes merch requests, content-artifact creation and show escalations as deterministic Discord actions.
- `promotion.budget` is intentionally **not advertised**: no trustworthy provider adapter exists in this export, so CrowdRelay fails closed instead of pretending the action happened.
- Calendar workflow is active in the intended production state. The reference JSON still contains the historical placeholder credential id; when updating through the UI, preserve/re-select the live Google Calendar OAuth credential already configured on the server. Advertise `calendar.upsert` only while that live binding is valid.
- Funding-package workflow is active in the intended production state. Keep `VIRYAOS_ENABLE_FUNDING_PACKAGE=1` only while `VIRYA_FUNDING_DRIVE_FOLDER_ID` and its Drive credential are valid; otherwise leave the workflow present but fail closed by withholding the capability.
- Team Ops capabilities are gated by `VIRYAOS_ENABLE_TEAM_OPS=1`. Enable only after the private verify/claim bridge routes the four manifest event types to `VOSTEAMOPS00001`.
- Team task email is shipped as the 57th workflow definition but remains inactive/fail-closed until `BRIDGE_ROUTE_DELTA.json` is applied, the live Gmail credential is bound, and `VIRYAOS_ENABLE_TEAM_EMAIL=1` is enabled; the heartbeat must advertise `team.email` only after those prerequisites are true.
- All VIRYA OS and verified-ingress workflows now disable success, failed and manual execution-data persistence.

Rollout order: **CrowdRelay backend first**, then import/activate n8n helpers/executors, update private bridge routes from `workflow-manifest.tsv`, then enable optional gates.

## 2026-08-11 lean production pass

- The previous lean source archive had **55 root workflows**. The current release restores Łazikomat as the active 56th workflow and adds `VIRYA — Team Task Email Executor` as the 57th definition; the new executor is imported inactive first and activated only after Gmail + bridge + capability-gate verification. Fourteen inactive/obsolete probes, reviews, scrapers and superseded automations live under `archive/` and are not imported by the root-only deploy path.
- `VOSMAIL000000001` no longer stores business/correlation state in workflow static data. Gmail `threadId` is resolved through CrowdRelay's provider-confirmed execution-receipt ledger; Gmail `messageId` is the inbound reply idempotency key.
- Cold outreach Gemini use is now per-message copy polishing with deterministic fallback. Policy, target selection and durable state stay in CrowdRelay.
- `VIRYA 11 — Entity Visibility (lean SEO)` replaces the synthetic 0–100 SEO cockpit. It checks only first-party entity integrity plus grounded search recognition/citation, emits at most two concrete recommendations, and uses no Google Sheets.
- New business logic belongs in CrowdRelay. n8n is restricted to provider adapters, provider-specific enrichment, scheduling/ingress and bounded technical delivery buffers.

Rollout dependency for this pack: **CrowdRelay 1.0.0 + migration 0042 first**, because the stateless mail reply monitor depends on the internal provider-correlation lookup; then import the n8n root production pack.
