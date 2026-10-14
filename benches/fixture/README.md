# `resolve` benchmark fixture

`benches/resolve.rs` times libpostal parsing and `Index::search` over the non-held-out rows of
`tests/addresses.tsv`, split by target. It needs libpostal's models and a small index, and does
nothing when `INDEX_DIR` is unset.

The index is built by `build.sh` from Overture release 2026-09-23.1, the OSM country extracts
the OpenAddresses layers `build_index` accepts by licence and the GeoNames `GB_full`, `CA_full` and
`NL_full` postal dumps (October 2026): every row inside the
0.08° boxes of `boxes.txt` (2,243 boxes around every `tests/gold.jsonl` point and every place the
address queries resolved to in a full index), through `build_index`. `slice_sources.py` cuts the
OSM, OpenAddresses and GeoNames files (osmium-tool and shapely); `build.sh` also keeps every
Overture division whose area touches a box, since a region's own row is a point far from the
boxes. It holds about 33 million house numbers and 1.8 million streets in 607 MB, so searches hit
real data for every row's country. Overture removes releases after a few months, so the result is published once as a
`tar.zst` release asset (`bench-fixture-v4`, 375 MB, SHA-256 in the repository variable), not
rebuilt in CI. Rebuild and republish it when `build_index` changes the index format
(`Index::open` then fails).

CI reads the asset's URL and checksum from the repository variables `BENCH_FIXTURE_URL` and
`BENCH_FIXTURE_SHA256`, caches it by checksum, and caches the libpostal models (about 1.8 GB).
Without the variables the CodSpeed workflow skips `resolve`.

```sh
cargo build --locked --release --bin build_index
OSM_DIR=~/geodata/osm OPENADDRESSES_DIR=~/geodata/openaddresses GEONAMES_DIR=~/geodata/geonames \
  benches/fixture/build.sh ~/geodata/overture/2026-09-23.1 /tmp/fixture   # about 12 minutes
tar --zstd -cf fixture.tar.zst -C /tmp/fixture index provenance.txt ATTRIBUTION.txt   # ATTRIBUTION.txt: see the release
INDEX_DIR=/tmp/fixture/index cargo bench --bench resolve   # wall-clock locally
```

Licensing: Overture's divisions and transportation themes and OSM are ODbL ("© OpenStreetMap
contributors"); Overture's addresses theme and each OpenAddresses layer have per-source licences. The artifact is a derived database:
keep the attribution with it. The index is a fixture for timing, not a source of expected answers.
Run against a fixture, the benchmark measures instruction counts under CodSpeed's simulation and
wall-clock time under `cargo bench`; neither is a request-latency guarantee.
