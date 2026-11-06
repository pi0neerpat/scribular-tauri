#!/usr/bin/env bash
# Add a build's cdhash to the partition list of Scribular's keychain items, so a rebuilt app reads
# its key without a prompt. A self-signed cert has no Team ID, so macOS partitions items by cdhash,
# which changes every build; the stable signature alone satisfies the ACL but not the partition list.
# `uitest launch` runs this automatically before every launch.
#
# One-time: bash tools/uitest/trust-build.sh --store-password
#   Saves the Mac login password in the login keychain (service scribular-uitest), readable by
#   /usr/bin/security without a prompt. Trade-off: any local process can read it that way.
# Usage: bash tools/uitest/trust-build.sh /path/to/Scribular.app
set -euo pipefail

KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
PW_SERVICE="scribular-uitest"
PW_ACCOUNT="login-password"

if [ "${1:-}" = "--store-password" ]; then
  read -rsp "Mac login password: " pw; echo
  # shortcut: checks the password only when the keychain is locked; the first trust run is the real check
  security unlock-keychain -p "$pw" "$KEYCHAIN" || { echo "wrong password" >&2; exit 1; }
  security add-generic-password -U -s "$PW_SERVICE" -a "$PW_ACCOUNT" -w "$pw" -T /usr/bin/security "$KEYCHAIN"
  echo "stored: builds will be trusted automatically"
  exit 0
fi

APP="${1:?usage: trust-build.sh /path/to/Scribular.app | --store-password}"
SERVICE=$(defaults read "$APP/Contents/Info" CFBundleIdentifier)
CDHASH=$(codesign -dvvv "$APP" 2>&1 | sed -n 's/^CDHash=//p')
[ -n "$CDHASH" ] || { echo "no CDHash: is $APP signed?" >&2; exit 1; }

# one line per item for $SERVICE: "<account>\t<current partition list>"
items=$(security dump-keychain -a "$KEYCHAIN" | awk -v svc="\"svce\"<blob>=\"$SERVICE\"" '
  function flush() { if (hit) print acct "\t" parts; hit = 0; acct = ""; parts = ""; want_parts = 0 }
  /^keychain: / { flush() }
  /authorizations.*partition_id/ { want_parts = 1 }
  index($0, svc) { hit = 1 }
  /"acct"<blob>="/ { acct = $0; sub(/.*"acct"<blob>="/, "", acct); sub(/"$/, "", acct) }
  want_parts && /description:/ { parts = $0; sub(/.*description: /, "", parts); gsub(/, /, ",", parts); want_parts = 0 }
  END { flush() }')
pw="${UITEST_KEYCHAIN_PASSWORD:-$(security find-generic-password -s "$PW_SERVICE" -a "$PW_ACCOUNT" -w "$KEYCHAIN" 2>/dev/null || true)}"
[ -n "$pw" ] || { echo "no login password: run 'bash tools/uitest/trust-build.sh --store-password' once" >&2; exit 1; }
[ -n "$items" ] || { echo "no keychain items for $SERVICE yet (first launch creates them)"; exit 0; }

while IFS=$'\t' read -r acct parts; do
  if [[ ",$parts," == *",cdhash:$CDHASH,"* ]]; then
    echo "already trusted: $SERVICE/$acct cdhash:$CDHASH"
    continue
  fi
  if ! printf '%s\n' "$pw" | security set-generic-password-partition-list -s "$SERVICE" -a "$acct" \
    -S "${parts:+$parts,}cdhash:$CDHASH" "$KEYCHAIN" > /dev/null; then
    echo "keychain rejected the stored password: re-run 'bash tools/uitest/trust-build.sh --store-password' (did your Mac login password change?)" >&2
    exit 1
  fi
  echo "trusted: $SERVICE/$acct cdhash:$CDHASH"
done <<< "$items"
