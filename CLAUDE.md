# FakApp (FAKAP)

Single-crate Rust watchdog (edition 2024). One static binary: probes targets,
shouts at Discord on transitions, serves the board. No database — JSON state
file.

## Layout
```
src/main.rs      boot: config, state restore, spawn checkers + saver, axum
src/config.rs    JSON config, env-sourced secrets, validation
src/state.rs     pure UP/DOWN state machine (thresholds, reminders) + tests
src/checker.rs   per-target probe loop, alert fan-out (discord + smtp)
src/discord.rs   webhook embeds; header = operator-set `alert_title`
src/mailer.rs    lettre SMTP transport (optional second channel)
src/store.rs     atomic JSON persistence + 60s checkpoint task
src/web.rs       server-rendered dashboard (/), /healthz, /llms.txt, /style.css
assets/          board.html template, style.css, llms.txt
deploy/          systemd unit, production config, Caddy example
scripts/         deploy.sh (virya-oracle), check_runtime_panics.py gate
```

## Must preserve
- FakApp observes only; it has no write path into anything it watches.
- **No `unwrap`/`expect`/`panic!`/`unreachable!` on runtime paths** (`unwrap_or*`
  is fine). `just panics` is the gate: a watchdog that panics dies quietly.
- Alerts are the product: at least one notifier (discord or smtp) must be
  configured, and a configured channel's secret must be present in env or
  config load fails loudly. Alert header is operator data (`alert_title`).
- Restart must not re-page about a known outage (state file is load-bearing).
- Board stays zero-JS server-rendered HTML; Lighthouse 100s are a gate.
- Binds loopback; TLS/auth belong to the edge proxy.

## Gates
```
just check    # fmt + clippy(-D warnings) + panics gate + test
just build    # release build, locked
just deploy   # static musl binary via docker -> install on virya-oracle -> verify
just rollback # previous binary back, restart, verify
```

## Deploy
Target: virya-oracle (`fakap.virya.music`) — deliberately a different box than
virya-crowdrelay, or the watchdog dies with the patient. The artifact is a
fully static musl binary built in an Alpine container locally; the tiny box
runs it bare under systemd with zero runtime dependencies.
Secrets live in `/etc/fakap/fakap.env` (`FAKAP_DISCORD_WEBHOOK_URL=...`,
optionally `FAKAP_SMTP_PASSWORD=...`); the service refuses to start without
them — a silent watchdog is decorative.
