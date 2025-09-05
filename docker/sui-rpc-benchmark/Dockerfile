# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.85-bullseye AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sui"

RUN apt update && apt install -y ca-certificates

RUN apt-get update && apt-get install -y cmake clang

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

RUN cargo build --profile ${PROFILE} --bin sui-rpc-benchmark

# Production Image
FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y libjemalloc-dev ca-certificates curl
ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so
WORKDIR "$WORKDIR/sui"
COPY --from=builder /sui/target/release/sui-rpc-benchmark /usr/local/bin
RUN apt update && apt install -y ca-certificates postgresql

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION