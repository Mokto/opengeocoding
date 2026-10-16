FROM rust:1.99.0-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    autoconf automake libtool pkg-config curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
ARG LIBPOSTAL_COMMIT=25099c506612b34b23b1bfe286ca6321fcf06f35
RUN mkdir /tmp/libpostal && curl -fsSL --retry 3 \
    "https://github.com/openvenues/libpostal/archive/${LIBPOSTAL_COMMIT}.tar.gz" \
    | tar -xz --strip-components=1 -C /tmp/libpostal \
    && cd /tmp/libpostal && ./bootstrap.sh \
    && flags="" && case "$(uname -m)" in aarch64|arm64) flags="--disable-sse2";; esac \
    && CFLAGS="-std=gnu99" ./configure --prefix=/usr/local --datadir=/data --disable-data-download $flags \
    && make -j"$(nproc)" && make install \
    && cp LICENSE /usr/local/lib/LICENSE.libpostal \
    && rm -rf /tmp/libpostal
ARG BUF_VERSION=1.68.1
RUN arch="$(uname -m)" && curl -fsSL --retry 3 \
    "https://github.com/bufbuild/buf/releases/download/v${BUF_VERSION}/buf-Linux-${arch}" \
    -o /usr/local/bin/buf && chmod +x /usr/local/bin/buf
WORKDIR /source
ENV RUSTFLAGS="-L native=/usr/local/lib"

# Empty stand-ins for every source file and a zeroed package version, so dependencies compile in a
# layer that only a change to dependencies, build.rs, proto/ or data/ invalidates, not a release bump.
FROM build AS stubs
COPY Cargo.toml Cargo.lock /stubs/
RUN sed -i '0,/^version = .*/s//version = "0.0.0"/' /stubs/Cargo.toml \
    && sed -i '/^name = "opengeocoding"$/{n;s/^version = .*/version = "0.0.0"/}' /stubs/Cargo.lock
COPY src src
COPY benches benches
RUN find src benches -name '*.rs' | while read -r f; do \
      mkdir -p "/stubs/$(dirname "$f")"; \
      case "$f" in \
        src/main.rs|src/bin/*.rs|src/bin/*/main.rs|benches/*.rs) echo 'fn main() {}' ;; \
        *) : ;; \
      esac > "/stubs/$f"; \
    done

FROM build AS manifest
COPY build.rs buf.yaml ./
COPY proto proto
COPY data data
COPY --from=stubs /stubs ./

FROM manifest AS deps
RUN cargo build --locked --release --bin opengeocoding --bin build_index --bin leader

FROM deps AS compiled
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY tests tests
COPY benches benches
COPY manifests manifests
# the stubs are newer than these files' timestamps, and cargo would trust them
RUN find src benches -name '*.rs' -exec touch {} + \
    && cargo build --locked --release --bin opengeocoding --bin build_index --bin leader

FROM compiled AS evaluate-compiled
RUN cargo build --locked --release --bin evaluate

FROM manifest AS check-deps
RUN ldconfig && rustup component add rustfmt clippy \
    && cargo clippy --locked --all-targets \
    && cargo test --locked --no-run

FROM check-deps AS checks
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY tests tests
COPY benches benches
COPY manifests manifests
RUN find src benches -name '*.rs' -exec touch {} + \
    && cargo fmt --check \
    && cargo clippy --locked --all-targets -- -D warnings \
    && cargo test --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1000 opengeocoding && useradd --uid 1000 --gid 1000 --create-home opengeocoding \
    && mkdir -p /app /data && chown 1000:1000 /app /data
COPY --from=build /usr/local/lib/libpostal.so* /usr/local/lib/
COPY --from=build /usr/local/lib/LICENSE.libpostal /usr/local/share/licenses/libpostal/LICENSE
COPY --from=build /usr/local/bin/libpostal_data /usr/local/bin/libpostal_data
COPY --from=compiled /source/target/release/opengeocoding /source/target/release/build_index /source/target/release/leader /usr/local/bin/
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.txt /usr/local/share/licenses/opengeocoding/
COPY LICENSES /usr/local/share/licenses/opengeocoding/dependencies/
RUN ldconfig
WORKDIR /app
USER 1000:1000
EXPOSE 11121 8000
ENTRYPOINT ["/usr/local/bin/opengeocoding"]

# The runtime image plus the evaluator and the gold corpus, for evaluation environments (the
# evaluator otherwise reads the corpus from its build directory). Not a release image.
FROM runtime AS eval
COPY --from=evaluate-compiled /source/target/release/evaluate /usr/local/bin/
# The gold suite as one file, written by the evaluator so the image and `evaluate run` agree: the
# curated corpus, then tests/registry/*.jsonl. The evaluator finds both under its build directory.
COPY tests/gold.jsonl /source/tests/gold.jsonl
COPY tests/registry /source/tests/registry
USER root
RUN mkdir -p /app/tests && evaluate gold-corpus --out /app/tests/gold.jsonl
USER 1000:1000
# The evaluator hashes the reference data from its build directory for the run manifest.
COPY data/reference.json /source/data/reference.json
