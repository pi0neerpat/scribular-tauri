#!/usr/bin/env bash
# After each build: add the new build's cdhash to the partition list of Scribular's keychain items.
# A self-signed cert has no Team ID, so macOS partitions items by cdhash, which changes every build;
# the stable signature alone satisfies the ACL but not the partition list.
# Usage: UITEST_KEYCHAIN_PASSWORD='<login password>' bash tools/uitest/trust-build.sh /path/to/Scribular.app
set -euo pipefail

APP="${1:?usage: trust-build.sh /path/to/Scribular.app}"
: "${UITEST_KEYCHAIN_PASSWORD:?set UITEST_KEYCHAIN_PASSWORD to the Mac login password}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
SERVICE=$(defaults read "$APP/Contents/Info" CFBundleIdentifier)
CDHASH=$(codesign -dvvv "$APP" 2>&1 | sed -n 's/^CDHash=//p')
[ -n "$CDHASH" ] || { echo "no CDHash: is $APP signed?" >&2; exit 1; }

# one block per item for $SERVICE: "<account>\t<current partition list>"
items=$(security dump-keychain -a "$KEYCHAIN" | awk -v svc="\"svce\"<blob>=\"$SERVICE\"" '
  function flush() { if (hit) print acct "\t" parts; hit = 0; acct = ""; parts = "" }
  /^keychain: / { flush() }
  index($0, svc) { hit = 1 }
  /"acct"<blob>="/ { acct = $0; sub(/.*"acct"<blob>="/, "", acct); sub(/"$/, "", acct) }
  /description: (apple-tool:|apple:|teamid:|cdhash:)/ { parts = $0; sub(/.*description: /, "", parts); gsub(/, /, ",", parts) }
  END { flush() }')
[ -n "$items" ] || { echo "no keychain items for $SERVICE yet: launch the app once first" >&2; exit 0; }

while IFS=$'\t' read -r acct parts; do
  if [[ ",$parts," == *",cdhash:$CDHASH,"* ]]; then
    echo "already trusted: $SERVICE/$acct cdhash:$CDHASH"
    continue
  fi
  security set-generic-password-partition-list -s "$SERVICE" -a "$acct" \
    -S "${parts:+$parts,}cdhash:$CDHASH" -k "$UITEST_KEYCHAIN_PASSWORD" "$KEYCHAIN" > /dev/null
  echo "trusted: $SERVICE/$acct cdhash:$CDHASH"
done <<< "$items"
