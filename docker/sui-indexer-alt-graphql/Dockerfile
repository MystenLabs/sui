# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
# rust:1.85-bullseye
FROM rust@sha256:033a8da97a1e4d53d51fcef8f8eefaba69ff2208fbee583c4ea84f704b307db2 AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sui"

# sui-indexer need ca-certificates
RUN apt update && apt install -y ca-certificates postgresql

RUN apt-get update && apt-get install -y cmake clang

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

RUN cargo build --profile ${PROFILE} --bin sui-indexer-alt-graphql

# Production Image
# debian:bullseye-slim
FROM debian@sha256:fdd75562fdcde1039c2480a1ea1cd2cf03b18b6e4cb551cabb03bde66ade8a5d AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y libjemalloc-dev ca-certificates curl
ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so
WORKDIR "$WORKDIR/sui"
COPY --from=builder /sui/target/release/sui-indexer-alt-graphql /usr/local/bin
RUN apt update && apt install -y ca-certificates postgresql

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
