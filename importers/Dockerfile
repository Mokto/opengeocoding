FROM rust:1.72.0

WORKDIR /app/src
RUN USER=root cargo new --bin opengeocoding_importers
COPY Cargo.toml Cargo.lock ./opengeocoding_importers/

WORKDIR /app/src/opengeocoding_importers
RUN cargo build --release

COPY ./ ./
RUN cargo build --release

FROM debian:stable-slim
WORKDIR /app
RUN apt update \
    && apt install -y openssl ca-certificates \
    && apt clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

COPY --from=builder \
    /app/src/opengeocoding_importers/target/release/geonames_import \
    /app/src/opengeocoding_importers/target/release/openaddresses_import \
    /app/src/opengeocoding_importers/target/release/openstreetmap_import \
    ./

CMD ["/app/openstreetmap_import"]