#!/usr/bin/env bash
# Builds the index fixture the `resolve` benchmark searches: the rows of a local Overture release
# (the layout README.md's index section downloads) inside the 0.08-degree boxes of boxes.txt,
# which surround every location of tests/gold.jsonl and every address tests/addresses.tsv
# resolves to, fed through `build_index`. Needs duckdb and a release build of build_index.
# Overture drops releases after a few months, so the result is published once as a release
# asset (see README.md); rebuild only when build_index changes the index format.
# usage: build.sh OVERTURE_RELEASE_DIR OUT_DIR   (writes OUT_DIR/index and OUT_DIR/provenance.txt)
set -euo pipefail
src=$(cd "$1" && pwd)
mkdir -p "$2"
out=$(cd "$2" && pwd)
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
rm -rf "$out/slice" "$out/index" "$out/.tmp"

# One duckdb process per file with its own temp directory: a single scan over the 67 GB of
# segments spilled hundreds of GB, and processes sharing a temp directory corrupt each other.
slice() {
  d=$1; f=$2; mkdir -p "$out/slice/$d" "$out/.tmp/$$"
  duckdb -c "SET preserve_insertion_order=false; SET threads=4; SET memory_limit='8GB'; SET temp_directory='$out/.tmp/$$';
    CREATE TABLE boxes AS SELECT * FROM read_csv('$here/boxes.txt', delim=' ', header=false, columns={'i':'INT','x1':'DOUBLE','y1':'DOUBLE','x2':'DOUBLE','y2':'DOUBLE'});
    COPY (SELECT t.* FROM read_parquet('$src/$d/$f') t WHERE EXISTS (SELECT 1 FROM boxes b WHERE t.bbox.xmin<=b.x2 AND t.bbox.xmax>=b.x1 AND t.bbox.ymin<=b.y2 AND t.bbox.ymax>=b.y1))
    TO '$out/slice/$d/$f' (FORMAT parquet, COMPRESSION zstd, COMPRESSION_LEVEL 9)"
  rm -rf "$out/.tmp/$$"
}
export -f slice
export src out here
for d in division division_area segment address; do
  ls "$src/$d" | xargs -P 4 -I{} bash -c "slice $d {}"
done

"$root/target/release/build_index" "$out/slice" "$out/index"
{
  echo "overture_release=$(basename "$src")"
  echo "boxes_sha256=$(shasum -a 256 "$here/boxes.txt" | cut -d' ' -f1)"
  echo "consumer_commit=$(git -C "$root" rev-parse HEAD)"
} > "$out/provenance.txt"
