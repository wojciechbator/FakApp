# FAKAP (FakApp)

<p align="center"><img src="assets/logo.svg" width="112" alt="FAKAP — the bomb with a heartbeat for a spark"></p>

A lightweight, single-binary watchdog: probes the virya services on an
interval, decides UP/DOWN with hysteresis, alerts Discord on every state
change (plus periodic reminders while anything stays down), and serves a
read-only status board.

Rust + axum. No database, no script, no build step — the dashboard is
server-rendered HTML with one stylesheet and zero JavaScript.

## Why

virya-crowdrelay going down must be noticed by a machine and reported to a
human within minutes, not noticed by a fan hours later. The board exists so
the same answer ("is it up?") is visible without reading mail.

## Run

```
cargo build --release
FAKAP_CONFIG=fakap.example.json FAKAP_SMTP_PASSWORD=... ./target/release/fakap
```

- `fakap.example.json` documents every field; copy it and edit the targets.
- SMTP credentials come from `FAKAP_SMTP_PASSWORD` (env or
  `/etc/fakap/smtp.env` via the unit file) so the config file stays
  secret-free.
- Without SMTP configured the checker still runs and the board still
  renders, but every config validation insists the password is present when
  an smtp block exists — a watchdog that cannot alert is decorative.

## Board

`/` renders one card per target: status badge, latency, uptime over retained
history, time in current state, last failure detail, and a strip of recent
probe outcomes. `/llms.txt` describes the service for agents. Lighthouse:
100/100/100/100 by construction (no JS, no images, semantic HTML,
indexable).

## State machine

Per target: N consecutive failures → DOWN (mail), M consecutive successes →
UP again (one all-clear mail), and while DOWN a reminder after
`repeat_alert_minutes`. A restart reloads the persisted state so it neither
re-pages about a known outage nor misses one that began while it was down.

## Gates

```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

## Deploy

Target host: the virya-oracle box (`fakap.virya.music`), which dies
independently of virya-crowdrelay — that separation is the point.

1. `cargo build --release`, ship `target/release/fakap` to
   `/usr/local/bin/fakap`.
2. Config to `/etc/fakap/fakap.json`; password into `/etc/fakap/smtp.env`
   as `FAKAP_SMTP_PASSWORD=...`.
3. `deploy/fakap.service` into `/etc/systemd/system/`,
   `systemctl daemon-reload && systemctl enable --now fakap`.
4. Caddy in front per `deploy/Caddyfile.example` (TLS terminates there).
