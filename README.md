# FAKAP

<p align="center"><img src="assets/logo.svg" width="112" alt="FAKAP — the bomb with a heartbeat for a spark"></p>

**A single-binary watchdog that probes your services and shouts when something is down.**

FAKAP monitors your infrastructure on an interval, decides UP/DOWN with hysteresis (a single blip is not an outage), alerts Discord on every state change, sends periodic reminders while anything stays down, and serves a read-only status board.

## What it does

- **Probes** targets on a configurable interval — HTTP health checks with timeout and threshold logic
- **Decides** UP/DOWN with hysteresis: a single failed check doesn't page; N consecutive failures do
- **Alerts** on every state transition (UP→DOWN, DOWN→UP) and sends reminders while anything stays down
- **Serves** a zero-JavaScript, server-rendered status board at `/` and a health endpoint at `/healthz`
- **Persists** state to a JSON file so a restart doesn't re-page about a known outage

## What it solves

A service going down must be noticed by a machine and reported to a human within minutes — not noticed by a customer hours later. FAKAP runs on a different box than the services it watches (so the watchdog doesn't die with the patient), uses a static binary with zero runtime dependencies, and stays quiet when everything is fine.

## Why a single binary

No database. No script. No build step on the target. The dashboard is server-rendered HTML with one stylesheet. The binary is fully static (musl) so it runs on any Linux box with no dependencies. Secrets come from environment variables or a systemd environment file — the config file stays secret-free.

## Run

```bash
cargo build --release
FAKAP_CONFIG=fakap.example.json FAKAP_DISCORD_WEBHOOK_URL=... ./target/release/fakap
```

- `fakap.example.json` documents every field — copy it and edit the targets
- Discord webhook URL comes from `FAKAP_DISCORD_WEBHOOK_URL` (env or `/etc/fakap/fakap.env`)
- Without Discord configured the checker still runs and the board still serves — a silent watchdog is decorative
# thomann-affiliate
