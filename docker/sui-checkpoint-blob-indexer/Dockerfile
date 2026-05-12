FROM rust:1.92-bullseye AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake clang \
 && rm -rf /var/lib/apt/lists/*
ARG PROFILE=release
WORKDIR /work

# COPYs ordered least- to most-frequently-changing for deeper layer cache hits.
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY external-crates external-crates
COPY sui-execution sui-execution
COPY consensus consensus
COPY crates crates

# sui-checkpoint-blob-indexer doesn't use GIT_REVISION at compile time, so we
# deliberately don't declare ARG/ENV GIT_REVISION in the builder — keeps the
# cargo-build layer cache key stable across commits with identical source.
RUN cargo build --locked --profile ${PROFILE} --bin sui-checkpoint-blob-indexer

FROM debian:12-slim AS deploy
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libjemalloc-dev \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder --chmod=755 /work/target/release/sui-checkpoint-blob-indexer /opt/sui/bin/sui-checkpoint-blob-indexer
