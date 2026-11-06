#!/usr/bin/env bash
# One-time: create a self-signed "Scribular Local Dev" codesigning identity in the login keychain.
# Builds signed with it keep a stable designated requirement, so keychain ACLs and TCC grants
# survive rebuilds. Idempotent. Nothing is written to the repo.
set -euo pipefail

NAME="Scribular Local Dev"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning | grep -q "$NAME"; then
  echo "already set up: \"$NAME\" is a valid codesigning identity"
  exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
pass=$(openssl rand -hex 16)

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -subj "/CN=$NAME" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" \
  -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null

# -legacy: macOS `security` cannot read OpenSSL 3+ default PKCS#12 encryption
legacy=""
if openssl version | grep -qE "^OpenSSL [3-9]"; then legacy="-legacy"; fi
openssl pkcs12 -export $legacy -inkey "$work/key.pem" -in "$work/cert.pem" \
  -name "$NAME" -passout "pass:$pass" -out "$work/id.p12"

security import "$work/id.p12" -k "$KEYCHAIN" -P "$pass" -T /usr/bin/codesign
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$work/cert.pem"

security find-identity -v -p codesigning | grep "$NAME"
echo
echo "Done. Build signed test apps with:"
echo "  APPLE_SIGNING_IDENTITY=\"$NAME\" npm run tauri -- build --bundles app"
