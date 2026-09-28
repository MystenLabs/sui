# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
ARG PROFILE=release-unwind
FROM rust:1.96.1-bookworm AS builder
ARG PROFILE
WORKDIR "$WORKDIR/sui"

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates cmake clang \
 && rm -rf /var/lib/apt/lists/*

# COPYs ordered least- to most-frequently-changing for deeper layer cache hits.
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY external-crates external-crates
COPY sui-execution sui-execution
COPY consensus consensus
COPY crates crates

# GIT_REVISION declared late so SHA changes don't invalidate the COPYs above.
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
RUN cargo build --locked --profile ${PROFILE} --bin sui-indexer-alt-jsonrpc

# Production Image
FROM debian:bookworm-slim AS runtime
ARG PROFILE
# Use jemalloc as memory allocator
RUN apt-get update \
 && apt-get install -y --no-install-recommends libjemalloc-dev ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so
WORKDIR "$WORKDIR/sui"
COPY --from=builder /sui/target/${PROFILE}/sui-indexer-alt-jsonrpc /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
