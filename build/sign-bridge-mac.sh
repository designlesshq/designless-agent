#!/usr/bin/env bash
#
# build/sign-bridge-mac.sh
#
# Signs and notarizes a single bridge binary using the Developer ID Application
# certificate. Invoked by .github/workflows/build-bridge.yml on the release path.
#
# Inputs (env):
#   MAC_CERT_P12          base64-encoded .p12 (Developer ID Application)
#   MAC_CERT_PASSWORD     password for the .p12
#   APPLE_ID              Apple ID for notarytool submission
#   APPLE_APP_PASSWORD    app-specific password
#   APPLE_TEAM_ID         Apple team ID; used as both the codesign identity
#                         and the notarytool team-id
#
# Argv:
#   $1                    absolute path to the binary to sign
#
# Outputs:
#   Signs the binary in-place. Notarizes via xcrun notarytool, waits for
#   the result, exits non-zero on failure.
#
# Note: CLI binaries don't support stapling (stapler only works on
# bundles/dmgs/installers). The notarization ticket is verified online by
# Gatekeeper when the binary is first launched from a quarantined location.

set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: $0 <binary-path>" >&2
  exit 64
fi

BINARY="$1"
if [ ! -f "$BINARY" ]; then
  echo "::error::binary not found: $BINARY" >&2
  exit 1
fi

if [ -z "${APPLE_TEAM_ID:-}" ]; then
  echo "::error::APPLE_TEAM_ID is not set" >&2
  exit 1
fi

RUNNER_TEMP="${RUNNER_TEMP:-$(mktemp -d)}"
KEYCHAIN="$RUNNER_TEMP/bridge-build.keychain-db"
KEYCHAIN_PASSWORD="$(openssl rand -hex 16)"
CERT_PATH="$RUNNER_TEMP/cert.p12"

cleanup() {
  security delete-keychain "$KEYCHAIN" 2>/dev/null || true
  rm -f "$CERT_PATH" 2>/dev/null || true
  rm -f "$RUNNER_TEMP/bridge.zip" 2>/dev/null || true
}
trap cleanup EXIT

# ── Import the signing certificate into an ephemeral keychain ──────────────
security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 7200 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"

echo "$MAC_CERT_P12" | base64 --decode > "$CERT_PATH"
security import "$CERT_PATH" \
  -k "$KEYCHAIN" \
  -P "$MAC_CERT_PASSWORD" \
  -T /usr/bin/codesign \
  -T /usr/bin/security

# Add our temporary keychain to the search list so codesign can find the cert.
ORIGINAL_KEYCHAINS=$(security list-keychains -d user | sed -e 's/"//g' | tr -s ' ')
# shellcheck disable=SC2086
security list-keychains -d user -s "$KEYCHAIN" $ORIGINAL_KEYCHAINS

# Allow codesign to use the key without an interactive prompt.
security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN"

# ── Sign with hardened runtime + secure timestamp (required for notarization) ──
echo "::group::codesign $BINARY"
codesign \
  --sign "$APPLE_TEAM_ID" \
  --options runtime \
  --timestamp \
  --force \
  --keychain "$KEYCHAIN" \
  "$BINARY"
codesign --verify --verbose=2 "$BINARY"
echo "::endgroup::"

# ── Notarize ────────────────────────────────────────────────────────────────
echo "::group::notarytool submit"
ZIP="$RUNNER_TEMP/bridge.zip"
ditto -c -k --keepParent "$BINARY" "$ZIP"

xcrun notarytool submit "$ZIP" \
  --apple-id "$APPLE_ID" \
  --password "$APPLE_APP_PASSWORD" \
  --team-id "$APPLE_TEAM_ID" \
  --wait
echo "::endgroup::"

echo "✓ signed + notarized $BINARY"
