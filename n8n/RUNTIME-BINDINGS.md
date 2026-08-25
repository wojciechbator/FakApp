# Runtime credential bindings

This release mirrors the intended production shape: the existing 56 workflows stay active and `VIRYA — Team Task Email Executor` is the only new workflow to import and activate after its Gmail binding is confirmed.

Do **not** mass-import all 57 workflow JSON files over production. Update only the files listed in `N8N_UI_UPDATE.md`.

## Google Calendar

The source archive predates the final Google Calendar OAuth binding made in production. `VOSCAL000000001.json` therefore still contains the placeholder credential id `REPLACE_WITH_GOOGLE_CALENDAR_CREDENTIAL`.

When updating this workflow through the n8n UI, preserve or re-select the already configured live `VIRYA Google Calendar` OAuth credential before saving/activating it. Do not replace the live credential with the placeholder from JSON.

## Gmail

`VIRYA — Team Task Email Executor` uses the same Gmail OAuth credential family as the existing VIRYA mail executor. After import, verify the Gmail node is bound to the live VIRYA Gmail credential before activation.

## Feature gates

Workflow activation and advertised executor capability are separate controls. Keep the existing runtime gates aligned with what is actually configured:

- `VIRYAOS_ENABLE_CALENDAR=1` only with the live Calendar OAuth binding.
- `VIRYAOS_ENABLE_FUNDING_PACKAGE=1` only when the funding Drive folder binding is valid.
- `VIRYAOS_ENABLE_TEAM_OPS=1` only when the team-ops provider branches are usable.
- `VIRYAOS_ENABLE_TEAM_EMAIL=1` only after `VIRYA — Team Task Email Executor` is imported, Gmail-bound, activated, and the private bridge route is updated.

The heartbeat advertises only enabled capabilities, so unavailable provider adapters fail closed instead of appearing operational.
