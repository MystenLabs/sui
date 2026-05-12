# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.92-bullseye AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake clang libpq5 libpq-dev postgresql ca-certificates \
 && rm -rf /var/lib/apt/lists/*
ARG PROFILE=release
WORKDIR /sui

# COPYs ordered least- to most-frequently-changing for deeper layer cache hits.
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY external-crates external-crates
COPY sui-execution sui-execution
COPY consensus consensus
COPY crates crates

# bridge-indexer doesn't use GIT_REVISION at compile time, so we deliberately
# don't declare ARG/ENV GIT_REVISION in the builder — keeps the cargo-build
# layer cache key stable across commits with identical source. The runtime
# stage below still takes GIT_REVISION as a build arg for the LABEL.
RUN cargo build --locked --profile ${PROFILE} --bin bridge-indexer

# Production Image
FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update \
 && apt-get install -y --no-install-recommends libjemalloc-dev ca-certificates curl libpq5 libpq-dev postgresql \
 && rm -rf /var/lib/apt/lists/*
ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so
WORKDIR /sui
COPY --from=builder /sui/target/release/bridge-indexer /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
