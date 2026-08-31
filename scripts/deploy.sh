#!/usr/bin/env bash
# Deploy FakApp to virya-oracle.
#
# The box is tiny, so the artifact is a statically linked musl binary built
# once in CI and downloaded by SHA — no local toolchain, no dependencies on
# the target. Each release lands in its own /srv/fakap/releases/<sha>/fakap
# directory; /usr/local/bin/fakap is an atomic symlink to the current one, so
# rollback is just re-pointing the symlink at the previous directory. A deploy
# that ends without a verified /healthz is a failure by definition.
#
# Required remote state:
#   /etc/fakap/fakap.json  monitoring config (seeded from
#                          deploy/fakap.production.json on first install)
#   /etc/fakap/fakap.env   secrets: FAKAP_DISCORD_WEBHOOK_URL=...
#                          (never written by deploys; the service refuses to
#                          start without it — silent watchdogs are useless)
#
# Usage:
#   scripts/deploy.sh <sha>            # download + install + verify
#   scripts/deploy.sh rollback         # re-symlink to previous release, verify
#
# Overrides: FAKAP_DEPLOY_HOST, FAKAP_DEPLOY_REMOTE_DIR, FAKAP_DEPLOY_SUDO,
# FAKAP_DEPLOY_ALLOW_DIRTY=1.
set -Eeuo pipefail
umask 077

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
REMOTE="${FAKAP_DEPLOY_HOST:-virya-oracle}"
REMOTE_DIR="${FAKAP_DEPLOY_REMOTE_DIR:-/srv/fakap}"
RELEASES_DIR="$REMOTE_DIR/releases"
SUDO="${FAKAP_DEPLOY_SUDO:-sudo}"
ALLOW_DIRTY="${FAKAP_DEPLOY_ALLOW_DIRTY:-0}"
LINK="/usr/local/bin/fakap"

fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }
require() { command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"; }
step() { printf '==> %s\n' "$*"; }

remote() { ssh "$REMOTE" "$@"; }
remote_root() {
  if [[ -n "$SUDO" ]]; then ssh "$REMOTE" "$SUDO" -n "$@"; else ssh "$REMOTE" "$@"; fi
}

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

cd "$ROOT_DIR"

if [[ "${1:-}" == "rollback" ]]; then
  step "Rolling back fakap on $REMOTE"
  current_sha="$(remote_root "readlink '$LINK'" 2>/dev/null | sed 's#'"$RELEASES_DIR"'/##; s#/fakap##' || true)"
  [[ -n "$current_sha" ]] || fail "cannot determine current release from $LINK symlink"
  # Use the recorded previous release file, falling back to the most recent
  # release directory that isn't the current one.
  prev_sha="$(remote_root "cat '$RELEASES_DIR/previous.txt' 2>/dev/null || true")"
  if [[ -z "$prev_sha" ]] || [[ "$prev_sha" == "$current_sha" ]]; then
    prev_sha="$(remote_root "ls -1 '$RELEASES_DIR'" 2>/dev/null | sort -r | grep -v "^${current_sha}\$" | head -1 || true)"
  fi
  [[ -n "$prev_sha" ]] || fail "no previous release directory in $RELEASES_DIR to roll back to"
  step "Re-pointing $LINK to $RELEASES_DIR/$prev_sha/fakap"
  remote_root "ln -s '$RELEASES_DIR/$prev_sha/fakap' '$LINK.tmp' && mv -Tf '$LINK.tmp' '$LINK'"
  remote_root 'systemctl restart fakap'
  verify
  exit 0
fi

SHA="${1:-}"
[[ -n "$SHA" ]] || fail "usage: scripts/deploy.sh <sha>   (or: scripts/deploy.sh rollback)"

[[ "$ALLOW_DIRTY" == "1" ]] || {
  [[ -z "$(git status --porcelain --untracked-files=normal)" ]] \
    || fail 'local worktree must be clean (or set FAKAP_DEPLOY_ALLOW_DIRTY=1)'
}
remote 'true' || fail "ssh host $REMOTE unreachable"
require gh

step "Downloading artifact fakap-binary-$SHA from GitHub Actions"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
gh run download -R "$(git remote get-url origin | sed 's#.*github.com[:/]##; s#\.git$##')" \
  -n "fakap-binary-$SHA" -D "$TMP/artifact" \
  || fail "could not download artifact fakap-binary-$SHA (is the publish workflow green?)"
BINARY="$TMP/artifact/fakap-${SHA}-linux-amd64"
[[ -f "$BINARY" ]] || fail "artifact did not contain fakap-${SHA}-linux-amd64"

step "Shipping to $REMOTE:$RELEASES_DIR/$SHA"
remote "mkdir -p '$RELEASES_DIR/$SHA'"
rsync -az "$BINARY" "$REMOTE:$RELEASES_DIR/$SHA/fakap"
rsync -az "$ROOT_DIR/deploy/" "$REMOTE:$REMOTE_DIR/deploy/"

step "Installing binary, unit and config"
remote_root "test -x '$RELEASES_DIR/$SHA/fakap'" \
  || fail "shipped binary does not run on $REMOTE (wrong arch?)"
remote_root "chmod 755 '$RELEASES_DIR/$SHA/fakap'"
# Verify the binary works BEFORE moving the symlink
remote "'$RELEASES_DIR/$SHA/fakap' --version" \
  || fail "binary fails --version before install (corrupted download?)"
# Atomic symlink swap: create temp symlink then rename over the old one
remote_root "ln -s '$RELEASES_DIR/$SHA/fakap' '$LINK.tmp' && mv -Tf '$LINK.tmp' '$LINK'" \
  || fail "failed to atomically swap symlink to new release"
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

verify

# Record the previous SHA for rollback (before this deploy's SHA)
prev_link="$(remote_root "readlink '$LINK'" 2>/dev/null || true)"
if [[ -n "$prev_link" ]]; then
  prev_sha_recorded="$(printf '%s' "$prev_link" | sed 's#'"$RELEASES_DIR"'/##; s#/fakap##')"
  if [[ -n "$prev_sha_recorded" ]] && [[ "$prev_sha_recorded" != "$SHA" ]]; then
    remote_root "printf '%s' '$prev_sha_recorded' > '$RELEASES_DIR/previous.txt'"
  fi
fi

printf '==> Done. Board: https://fakap.virya.music/\n'
