FROM rust:1.92-bookworm AS builder

WORKDIR /build
COPY . .
RUN cargo build --release --bin gcs_scanner

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/gcs_scanner /gcs_scanner

RUN mkdir -p /data
VOLUME ["/data"]

ENTRYPOINT ["/gcs_scanner"]
