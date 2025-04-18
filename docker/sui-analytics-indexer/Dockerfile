FROM rust:1.85-bullseye AS builder

ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sui"

RUN apt-get update && apt-get install -y cmake clang

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

RUN RUSTFLAGS="-C debuginfo=2" cargo build --profile ${PROFILE} --bin sui-analytics-indexer

# Production Image
FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y \
  libjemalloc-dev \
  ca-certificates \
  procps \
  htop \
  binutils \
  lsof \
  heaptrack
ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so

WORKDIR "$WORKDIR/sui"
COPY --from=builder /sui/target/release/sui-analytics-indexer /opt/sui/bin/sui-analytics-indexer

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
