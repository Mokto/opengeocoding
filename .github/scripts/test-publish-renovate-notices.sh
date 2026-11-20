#!/usr/bin/env bash
set -euo pipefail

publish="$(cd "$(dirname "$0")" && pwd)/publish-renovate-notices.sh"
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT

git init -q --bare "$tmp/remote"
git init -q -b main "$tmp/repo"
cd "$tmp/repo"
git config user.name Test
git config user.email test@example.org
git config commit.gpgsign false
git remote add origin "file://$tmp/remote"
printf 'base notices\n' > THIRD_PARTY_NOTICES.txt
git add THIRD_PARTY_NOTICES.txt
git commit -qm base
base=$(git rev-parse HEAD)
branch=renovate/test
ref="refs/heads/$branch"
git switch -qc "$branch"
printf 'updated dependencies\n' > Cargo.lock
git add Cargo.lock
git commit -qm update
head=$(git rev-parse HEAD)
git push -q origin main "$ref"
git -C "$tmp/remote" symbolic-ref HEAD refs/heads/main
git clone -q --depth=1 --no-tags "file://$tmp/remote" "$tmp/publisher"
cd "$tmp/publisher"
git config commit.gpgsign false

printf 'new notices\n' > "$tmp/notices"
bash "$publish" "$branch" "$base" "$tmp/notices"
[[ $(git ls-remote origin "$ref" | cut -f1) == "$head" ]]
bash "$publish" "$branch" "$head" "$tmp/notices"
git fetch -q origin "$ref"
published=$(git rev-parse FETCH_HEAD)
[[ $(git rev-parse "$published^") == "$head" ]]
[[ $(git diff --name-only "$head" "$published") == THIRD_PARTY_NOTICES.txt ]]
git show "$published:THIRD_PARTY_NOTICES.txt" | cmp - "$tmp/notices"
[[ $(git rev-parse HEAD) == "$base" && -z $(git status --porcelain) ]]

bash "$publish" "$branch" "$published" "$tmp/notices"
[[ $(git ls-remote origin "$ref" | cut -f1) == "$published" ]]
