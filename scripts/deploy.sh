#!/usr/bin/env bash
# Deploy FakApp to virya-oracle.
#
# The box is tiny, so the artifact is a statically linked musl binary built
# once (in an Alpine rust container) and shipped as a single file — no
# toolchain, no dependencies on the target. Install = swap one binary,
# restart systemd, verify the board actually answers. A deploy that ends
# without a verified /healthz is a failure by definition.
#
# Required remote state:
#   /etc/fakap/fakap.json  monitoring config (seeded from
#                          deploy/fakap.production.json on first install)
#   /etc/fakap/fakap.env   secrets: FAKAP_DISCORD_WEBHOOK_URL=...
#                          (never written by deploys; the service refuses to
#                          start without it — silent watchdogs are useless)
#
# Usage:
#   scripts/deploy.sh            # build + install + verify
#   scripts/deploy.sh rollback   # restore previous binary, restart, verify
#
# Overrides: FAKAP_DEPLOY_HOST, FAKAP_DEPLOY_REMOTE_DIR, FAKAP_DEPLOY_SUDO,
# FAKAP_DEPLOY_ALLOW_DIRTY=1, FAKAP_SKIP_BUILD=1 (reuse dist-staging binary).
set -Eeuo pipefail
umask 077

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
REMOTE="${FAKAP_DEPLOY_HOST:-virya-oracle}"
REMOTE_DIR="${FAKAP_DEPLOY_REMOTE_DIR:-/srv/fakap}"
SUDO="${FAKAP_DEPLOY_SUDO:-sudo}"
ALLOW_DIRTY="${FAKAP_DEPLOY_ALLOW_DIRTY:-0}"
STAGING="$ROOT_DIR/dist-staging"
BINARY="$STAGING/fakap-linux-amd64"

fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }
require() { command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"; }
step() { printf '==> %s\n' "$*"; }

remote() { ssh "$REMOTE" "$@"; }
remote_root() {
  if [[ -n "$SUDO" ]]; then ssh "$REMOTE" "$SUDO" -n "$@"; else ssh "$REMOTE" "$@"; fi
}

cd "$ROOT_DIR"

if [[ "${1:-deploy}" == "rollback" ]]; then
  step "Rolling back fakap on $REMOTE"
  remote_root 'test -f /usr/local/bin/fakap.previous' \
    || fail "no previous binary on $REMOTE to roll back to"
  remote_root 'cp /usr/local/bin/fakap.previous /usr/local/bin/fakap'
  remote_root 'systemctl restart fakap'
  verify
  exit 0
fi

require docker
[[ "$ALLOW_DIRTY" == "1" ]] || {
  [[ -z "$(git status --porcelain --untracked-files=normal)" ]] \
    || fail 'local worktree must be clean (or set FAKAP_DEPLOY_ALLOW_DIRTY=1)'
}
remote 'true' || fail "ssh host $REMOTE unreachable"

build_static() {
  step "Building static musl binary in Alpine container (linux/amd64)"
  mkdir -p "$STAGING"
  docker run --rm --platform linux/amd64 \
    -v "$ROOT_DIR":/io -w /io \
    -e CARGO_TARGET_DIR=/io/target-musl \
    rust:1-alpine \
    sh -c 'RUSTFLAGS="-C target-feature=+crt-static" cargo build --release --locked && cp target-musl/release/fakap /io/dist-staging/fakap-linux-amd64'
  [[ -f "$BINARY" ]] || fail "static build produced no binary"
  file "$BINARY" | grep -q 'statically linked' \
    || fail "binary is not fully static: $(file "$BINARY")"
}

if [[ "${FAKAP_SKIP_BUILD:-0}" == "1" && -f "$BINARY" ]]; then
  step "Skipping build (FAKAP_SKIP_BUILD=1), reusing $BINARY"
else
  build_static
fi

step "Shipping to $REMOTE:$REMOTE_DIR"
remote "mkdir -p '$REMOTE_DIR'"
rsync -az "$BINARY" "$REMOTE:$REMOTE_DIR/fakap.new"
rsync -az "$ROOT_DIR/deploy/" "$REMOTE:$REMOTE_DIR/deploy/"

step "Installing binary, unit and config"
remote_root "test -x '$REMOTE_DIR/fakap.new'" || fail "shipped binary does not run on $REMOTE (wrong arch?)"
remote_root "install -m755 '$REMOTE_DIR/fakap.new' /usr/local/bin/fakap.new"
remote_root 'test -f /usr/local/bin/fakap && cp /usr/local/bin/fakap /usr/local/bin/fakap.previous || true'
remote_root 'mv /usr/local/bin/fakap.new /usr/local/bin/fakap'
remote "/usr/local/bin/fakap --version" || fail "binary fails --version after install"
remote_root "install -Dm644 '$REMOTE_DIR/deploy/fakap.service' /etc/systemd/system/fakap.service"
if ! remote_root 'test -f /etc/fakap/fakap.json'; then
  step "First install: seeding /etc/fakap/fakap.json from production example"
  remote_root "install -Dm600 '$REMOTE_DIR/deploy/fakap.production.json' /etc/fakap/fakap.json"
fi
if ! remote_root 'test -f /etc/fakap/fakap.env'; then
  fail "/etc/fakap/fakap.env is missing on $REMOTE — create it with one line:
  FAKAP_DISCORD_WEBHOOK_URL=https://discord.com/api/webhooks/<id>/<token>
A watchdog that cannot alert will not be started."
fi

step "(Re)starting service"
remote_root 'systemctl daemon-reload'
remote_root 'systemctl enable fakap >/dev/null 2>&1 || true'
remote_root 'systemctl restart fakap'
sleep 2

verify() {
  step "Verifying"
  remote_root 'systemctl is-active --quiet fakap' \
    || { remote_root 'journalctl -u fakap -n 20 --no-pager'; fail "fakap is not active"; }
  remote 'curl -fsS http://127.0.0.1:8183/healthz' >/dev/null \
    || fail "local healthz did not answer on $REMOTE"
  local banner
  banner="$(remote 'curl -fsS http://127.0.0.1:8183/' | grep -o 'ALL SYSTEMS GO\|SERVICE[S]* DOWN\|NO TARGETS CONFIGURED' | head -1)" || true
  if curl -fsS --max-time 10 https://fakap.virya.music/healthz >/dev/null 2>&1; then
    printf 'EDGE=https://fakap.virya.music/healthz OK\n'
  else
    printf 'EDGE=unreachable (check the edge proxy for fakap.virya.music)\n'
  fi
  printf 'DEPLOY=OK host=%s board=%s\n' "$REMOTE" "${banner:-unreadable}"
}
verify

printf '==> Done. Board: https://fakap.virya.music/\n'
