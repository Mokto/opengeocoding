#!/usr/bin/env bash
# Opens an issue for each new stable release of the crates in the builder hash.
# DRY_RUN=1 prints the issues instead of opening them.
set -euo pipefail

for crate in tantivy tantivy-fst deunicode; do
  current=$(awk -v name="name = \"$crate\"" '$0 == name { getline; gsub(/version = |"/, ""); print; exit }' Cargo.lock)
  [[ -n $current ]] || { echo "$crate is not in Cargo.lock" >&2; exit 1; }
  info=$(curl -fsS --retry 3 -A "opengeocoding index dependency releases (github.com/Mokto/opengeocoding)" \
    "https://crates.io/api/v1/crates/$crate")
  latest=$(jq -r .crate.max_stable_version <<<"$info")
  if [[ $latest == "$current" || $(printf '%s\n' "$current" "$latest" | sort -V | tail -1) != "$latest" ]]; then
    echo "$crate: $current is current"
    continue
  fi
  title="Upgrade $crate to $latest"
  if [[ -n $(gh issue list --state all --search "\"$title\" in:title" --json title --jq ".[] | select(.title == \"$title\") | .title") ]]; then
    echo "$crate: an issue for $latest exists"
    continue
  fi

  repo=$(jq -r '.crate.repository // ""' <<<"$info" | sed -E 's#^https?://github\.com/##; s#\.git$##; s#/$##')
  notes=""
  if [[ $repo =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
    for tag in "$latest" "v$latest" "$crate-v$latest"; do
      if notes=$(gh release view "$tag" -R "$repo" --json body --jq .body 2>/dev/null); then break; fi
      notes=""
    done
    # Without a GitHub release, the version's section of CHANGELOG.md: from the heading (markdown
    # or underlined) naming it to the next heading naming a version.
    [[ -n $notes ]] || notes=$(curl -fsSL --retry 3 "https://raw.githubusercontent.com/$repo/HEAD/CHANGELOG.md" 2>/dev/null |
      awk -v v="$latest" '
        { line[NR] = $0 }
        END {
          for (i = 1; i <= NR; i++) {
            if (!(line[i] ~ /^#/ || line[i + 1] ~ /^(=+|-+)$/) || line[i] !~ /[0-9]+\.[0-9]+/) continue
            if (start) { end = i - 1; break }
            if (index(line[i], v)) start = i
          }
          if (start) for (i = start; i <= (end ? end : NR); i++) print line[i]
        }') || notes=""
    links="[Releases](https://github.com/$repo/releases) · [CHANGELOG.md](https://github.com/$repo/blob/HEAD/CHANGELOG.md) · [crates.io](https://crates.io/crates/$crate/$latest)"
  else
    links="[crates.io](https://crates.io/crates/$crate/$latest)"
  fi

  body="\`$crate\` $latest is out; \`Cargo.lock\` has $current.

$links

## Release notes

${notes:-No release notes found for $latest; see the links above.}

## Upgrading

\`$crate\` is in the builder hash, so upgrading it names a new index: rebuild it, run the gold diff against the current index, and update \`index.builderHash\` in \`manifests/values.yaml\` in the same commit."
  body=${body:0:60000}

  if [[ -n ${DRY_RUN:-} ]]; then
    printf '=== %s\n%s\n\n' "$title" "$body"
  else
    gh issue create --title "$title" --body "$body"
  fi
done
