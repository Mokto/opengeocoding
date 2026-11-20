#!/usr/bin/env bash
set -euo pipefail

branch=$1 sha=$2 notices=$3
ref="refs/heads/$branch"
[[ $branch == renovate/?* && $sha =~ ^[0-9a-f]{40}$ && -s $notices && -f $notices && ! -L $notices ]]
git check-ref-format "$ref"

git fetch --no-tags --depth=1 origin "$ref"
if [[ $(git rev-parse FETCH_HEAD) != "$sha" ]]; then
  echo "Renovate moved $branch; leaving its new head alone"
  exit 0
fi

# Do not check out the PR or run its Git filters/hooks with a write credential.
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
export GIT_INDEX_FILE="$scratch/index"
git read-tree "$sha"
blob=$(git hash-object -w --no-filters -- "$notices")
git update-index --add --cacheinfo "100644,$blob,THIRD_PARTY_NOTICES.txt"
tree=$(git write-tree)
[[ $tree != "$(git rev-parse "$sha^{tree}")" ]] || exit 0
commit=$(git -c user.name='OpenGeocoding Notices Bot' -c user.email='opengeocoding-notices-bot@users.noreply.github.com' \
  commit-tree "$tree" -p "$sha" -m 'chore: regenerate third-party notices')

# Require the exact old head even if Renovate rewrites the branch back to an ancestor.
git -c credential.helper= -c 'credential.helper=!gh auth git-credential' \
  push --force-with-lease="$ref:$sha" origin "$commit:$ref"
