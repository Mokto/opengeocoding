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
    && make -j2 && make install \
    && cp LICENSE /usr/local/lib/LICENSE.libpostal \
    && rm -rf /tmp/libpostal
ARG BUF_VERSION=1.68.1
RUN arch="$(uname -m)" && curl -fsSL --retry 3 \
    "https://github.com/bufbuild/buf/releases/download/v${BUF_VERSION}/buf-Linux-${arch}" \
    -o /usr/local/bin/buf && chmod +x /usr/local/bin/buf
WORKDIR /source
COPY Cargo.toml Cargo.lock build.rs buf.yaml ./
COPY proto proto
COPY data data
COPY src src
COPY tests tests
COPY benches benches
COPY manifests manifests
ENV RUSTFLAGS="-L native=/usr/local/lib"
RUN cargo build --locked --release --bin opengeocoding --bin build_index

FROM build AS checks
RUN ldconfig && rustup component add rustfmt clippy \
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
COPY --from=build /source/target/release/opengeocoding /source/target/release/build_index /usr/local/bin/
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.txt /usr/local/share/licenses/opengeocoding/
COPY LICENSES /usr/local/share/licenses/opengeocoding/dependencies/
RUN ldconfig
WORKDIR /app
USER 1000:1000
EXPOSE 11121 8000
ENTRYPOINT ["/usr/local/bin/opengeocoding"]
