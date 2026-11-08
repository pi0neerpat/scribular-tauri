#!/usr/bin/env bash
set -euo pipefail

base=${1:?usage: check-additive.sh <base-ref>}
git rev-parse --verify "$base^{commit}" >/dev/null

while IFS=$'\t' read -r added removed path; do
  [[ -n ${path:-} ]] || continue
  case "$path" in
    tools/uitest/Tests/*|tools/uitest/scenarios/*.uitest)
      if git cat-file -e "$base:$path" 2>/dev/null && [[ ${removed:-0} != 0 ]]; then
        printf 'non-additive change in %s (removed %s lines)\n' "$path" "$removed" >&2
        exit 1
      fi
      ;;
  esac
done < <(git diff --no-renames --numstat "$base...HEAD" -- tools/uitest/Tests tools/uitest/scenarios)

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git show "$base:tools/uitest/README.md" | awk '
  /^\| `launch / { inside=1 }
  inside && /^\| `/ { print }
  /^\| `quit` \|/ { exit }
' > "$tmp/base-command-rows"
git show HEAD:tools/uitest/README.md | awk '
  /^\| `launch / { inside=1 }
  inside && /^\| `/ { print }
  /^\| `quit` \|/ { exit }
' > "$tmp/head-command-rows"
while IFS= read -r row; do
  if ! grep -F -x -q -- "$row" "$tmp/head-command-rows"; then
    printf 'changed or removed existing README command row: %s\n' "$row" >&2
    exit 1
  fi
done < "$tmp/base-command-rows"
printf 'additive uitest changes: ok\n'
