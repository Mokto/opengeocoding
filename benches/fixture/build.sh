#!/usr/bin/env bash
# Builds the index fixture the `resolve` benchmark searches: Overture rows inside the boxes of
# boxes.txt (0.08-degree squares around places that the benchmark's queries resolve to, picked
# with a full index), fed through `build_index`. Needs duckdb, network access to Overture's S3
# bucket and a release build of build_index.
# Overture drops releases after a few months, so the result is published once as an artifact
# (see README.md); rebuild only when build_index changes the index format.
# usage: build.sh OUT_DIR   (writes OUT_DIR/index and OUT_DIR/provenance.txt)
set -eu
RELEASE=2026-09-23.1
here=$(cd "$(dirname "$0")" && pwd)
out=$(cd "$1" 2>/dev/null && pwd || { mkdir -p "$1" && cd "$1" && pwd; })
root=$(cd "$here/../.." && pwd)
rm -rf "$out/parts" "$out/slice" "$out/index"

slice() {
  i=$1; x1=$2; y1=$3; x2=$4; y2=$5
  w="bbox.xmin<=$x2 AND bbox.xmax>=$x1 AND bbox.ymin<=$y2 AND bbox.ymax>=$y1"
  for t in divisions/division:division divisions/division_area:division_area \
           transportation/segment:segment addresses/address:address; do
    src=${t%%:*}; d=${t##*:}; mkdir -p "$out/parts/$d"
    duckdb -c "INSTALL httpfs; LOAD httpfs; SET s3_region='us-west-2'; COPY (SELECT * FROM read_parquet('s3://overturemaps-us-west-2/release/$RELEASE/theme=${src%%/*}/type=${src##*/}/*') WHERE $w) TO '$out/parts/$d/b$i.parquet' (FORMAT parquet, COMPRESSION zstd)" >/dev/null
  done
}
export out RELEASE
# ponytail: one S3 scan per box and theme, 8 at a time (about 20 minutes); a single scan with
# an OR of all boxes defeats row-group pruning.
export -f slice
xargs -P 8 -L 1 bash -c 'slice "$@"' _ < "$here/boxes.txt"

for d in division division_area segment address; do
  mkdir -p "$out/slice/$d"
  duckdb -c "COPY (SELECT * EXCLUDE rn FROM (SELECT *, row_number() OVER (PARTITION BY id) rn FROM read_parquet('$out/parts/$d/*.parquet')) WHERE rn=1) TO '$out/slice/$d/part-0.parquet' (FORMAT parquet, COMPRESSION zstd, COMPRESSION_LEVEL 9)"
done
"$root/target/release/build_index" "$out/slice" "$out/index"
{
  echo "overture_release=$RELEASE"
  echo "boxes_sha256=$(shasum -a 256 "$here/boxes.txt" | cut -d' ' -f1)"
  echo "consumer_commit=$(git -C "$root" rev-parse HEAD)"
} > "$out/provenance.txt"
