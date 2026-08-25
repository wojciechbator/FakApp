# n8n UI update — final VIRYA ecosystem release

Do not mass-import the whole n8n directory. The current production instance already contains the existing 56 workflows and newer live OAuth bindings.

## Update these existing workflows

1. `CrowdRelay — VIRYA OS opportunity application executor` (`VOSAPP000000001`)
   - adds provider execution claim before the external provider side effect;
   - ambiguous provider failures fail closed instead of blindly retrying.

2. `CrowdRelay — VIRYA OS Calendar executor` (`VOSCAL000000001`)
   - adds provider-confirmed success receipt;
   - preserves deterministic Calendar event identity;
   - **after import/replacement re-select the existing live `VIRYA Google Calendar` OAuth credential** because the JSON contains the historical placeholder credential id.

3. `CrowdRelay — VIRYA OS funding package executor` (`VOSFUND00000001`)
   - adds provider execution claim and fail-closed ambiguous outcome handling.

4. `CrowdRelay — VIRYA OS funding submission executor` (`VOSFUND00000002`)
   - adds provider execution claim and fail-closed ambiguous outcome handling.

5. `CrowdRelay — VIRYA OS executor heartbeat` (`VOSHEARTBEAT001`)
   - advertises exact capability surface;
   - adds gated `team.email` capability.

6. `CrowdRelay — VIRYA OS mail executor + daily Gemini polish` (`VOSMAIL000000001`)
   - adds provider execution claim before Gmail send;
   - ambiguous Gmail/provider outcomes fail closed to avoid duplicate sends.

7. `CrowdRelay — VIRYA OS execution receipt spooler` (`VOSRECEIPT00001`)
   - carries the execution claim token into the terminal report;
   - keeps receipt delivery retryable without replaying provider success.

8. `CrowdRelay — VIRYA OS team action executor` (`VOSTEAMOPS00001`)
   - adds provider execution claim before non-idempotent provider actions;
   - keeps deterministic team-action shaping and provider-confirmed receipts.

## Add this new workflow

9. `VIRYA — Team Task Email Executor` (`VOSTEAMEMAIL001`)
   - import as new workflow;
   - bind its Gmail node to the live VIRYA Gmail OAuth credential;
   - activate it only after the bridge route below exists;
   - then enable `VIRYAOS_ENABLE_TEAM_EMAIL=1` in the n8n runtime and let `VOSHEARTBEAT001` refresh.

## Do not update

- `VIRYA 21 — Łazikomat FB + IG + 7D Unfollow — Parse` (`sjH0yjbPB1dEN19e`) is included in the full reference pack as active 56th existing workflow, but there is no code change to apply through the UI.
- The verified ingress workflow (`44808b183edf4ac7`) itself is unchanged.

## One non-workflow runtime change is required

The private verify/claim bridge must route:

`viryaos.team.assignment_email_requested -> VOSTEAMEMAIL001`

The exact delta is in `BRIDGE_ROUTE_DELTA.json`. Do not enable the `team.email` heartbeat capability until this route exists, otherwise CrowdRelay can emit a valid action that ingress cannot dispatch.
