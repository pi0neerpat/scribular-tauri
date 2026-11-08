#!/usr/bin/env bash
set -euo pipefail

script=$(cd "$(dirname "$0")" && pwd)/check-additive.sh
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
git init -q
mkdir -p tools/uitest/Tests tools/uitest/scenarios
printf 'test\n' > tools/uitest/Tests/Existing.swift
printf 'launch\nquit\n' > tools/uitest/scenarios/existing.uitest
cat > tools/uitest/README.md <<'EOF'
| Command | Meaning |
| --- | --- |
| `launch` | Start. |
| `quit` | Quit. |
EOF
git add .
GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid git commit -qm base
base=$(git rev-parse HEAD)
printf 'new\n' > tools/uitest/scenarios/new.uitest
printf '| `new-command` | Additive. |\n' >> tools/uitest/README.md
git add .
GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid git commit -qm additive
bash "$script" "$base"
printf 'changed\n' > tools/uitest/Tests/Existing.swift
git add .
GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid git commit -qm modifying
if bash "$script" HEAD~1; then
  echo 'expected modifying commit to fail additive guard' >&2
  exit 1
fi
echo 'additive self-test: ok'
