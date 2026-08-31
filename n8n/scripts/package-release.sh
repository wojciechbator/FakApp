#!/usr/bin/env bash
# Package all n8n workflow JSONs + release manifest into a versioned artifact.
#
# Produces n8n-release-<SHA>.tar.zst and prints its SHA256. The artifact is
# the exact set of workflows that push-workflows.py applied in that release.
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT_DIR"

require() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1" >&2; exit 1; }; }
require git
require tar

SHA="$(git rev-parse --short HEAD)"
ARTIFACT="n8n-release-${SHA}.tar.zst"

STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

mkdir -p "$STAGING/workflows"
cp -- *.json "$STAGING/workflows/" 2>/dev/null || true
[[ -f release-manifest.json ]] && cp release-manifest.json "$STAGING/"
[[ -f routes.json ]] && cp routes.json "$STAGING/"

tar --zstd -cf "$ARTIFACT" -C "$STAGING" .
SHA256="$(shasum -a 256 "$ARTIFACT" | awk '{print $1}')"

printf 'ARTIFACT=%s sha256=%s workflows=%d\n' \
  "$ARTIFACT" "$SHA256" \
  "$(find "$STAGING/workflows" -name '*.json' | wc -l | tr -d ' ')"
