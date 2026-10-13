# `resolve` benchmark fixture

`benches/resolve.rs` times libpostal parsing and `Index::search` over the non-held-out rows of
`tests/addresses.tsv` for Denmark, the Netherlands, France, Switzerland, Norway and Spain. It
needs libpostal's models and a small index, and does nothing when `INDEX_DIR` is unset.

The index is built by `build.sh` from Overture release 2026-09-23.1: every row inside the 0.08°
boxes of `boxes.txt` (77 boxes around the places those queries resolved to in a full index),
through `build_index`. It holds about 4,500 places and 1.6 million house numbers in 27 MB.
Overture removes releases after a few months, so the result is published once as a
`tar.zst` release asset (`bench-fixture-v1`, SHA-256 in the repository variable), not rebuilt in CI. Rebuild and republish it when `build_index` changes the
index format (`Index::open` then fails).

CI reads the artifact's URL and checksum from the repository variables `BENCH_FIXTURE_URL` and
`BENCH_FIXTURE_SHA256`, and caches the libpostal models (about 1.8 GB). Without the variables the
CodSpeed workflow skips `resolve`.

```sh
cargo build --locked --release --bin build_index
benches/fixture/build.sh /tmp/fixture            # about 20 minutes
tar --zstd -cf fixture.tar.zst -C /tmp/fixture index provenance.txt ATTRIBUTION.txt   # ATTRIBUTION.txt: see the release
INDEX_DIR=/tmp/fixture/index cargo bench --bench resolve   # wall-clock locally
```

Licensing: Overture's divisions and transportation themes are ODbL ("© OpenStreetMap
contributors"); its addresses theme has per-source licences. The artifact is a derived database:
keep the attribution with it. The index is a fixture for timing, not a source of expected answers.
Run against a fixture, the benchmark measures instruction counts under CodSpeed's simulation and
wall-clock time under `cargo bench`; neither is a request-latency guarantee.
