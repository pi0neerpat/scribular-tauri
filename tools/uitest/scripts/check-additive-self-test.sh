#!/usr/bin/env bash
set -euo pipefail

script=$(cd "$(dirname "$0")" && pwd)/check-additive.sh
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
git init -q
mkdir -p tools/uitest/Tests/UITestCoreTests tools/uitest/scenarios
printf 'test\n' > tools/uitest/Tests/Existing.swift
printf '@Test func one() {}\n@Test func two() {}\n' > tools/uitest/Tests/UITestCoreTests/PlayTests.swift
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
awk '/\| `quit` \|/ { print "| `new-command` | Additive. |" } { print }' tools/uitest/README.md > README.tmp
mv README.tmp tools/uitest/README.md
git add .
GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid git commit -qm additive
bash "$script" "$base"
sed -i '' 's/@Test func one() {}/@Test func one() { #expect(true) }/' tools/uitest/Tests/UITestCoreTests/PlayTests.swift
git add tools/uitest/Tests/UITestCoreTests/PlayTests.swift
GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid git commit -qm 'replace test while preserving count'
bash "$script" "$base"
printf 'changed\n' > tools/uitest/Tests/Existing.swift
git add .
GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid git commit -qm modifying
if bash "$script" HEAD~1; then
  echo 'expected modifying commit to fail additive guard' >&2
  exit 1
fi
sed -i '' 's/| `launch` | Start. |/| `launch` | Changed. |/' tools/uitest/README.md
git add tools/uitest/README.md
GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid git commit -qm 'modify README command row'
if bash "$script" HEAD~1; then
  echo 'expected README command row modification to fail additive guard' >&2
  exit 1
fi
echo 'additive self-test: ok'
