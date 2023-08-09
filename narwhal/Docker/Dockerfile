# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.70.0 AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sui"
RUN apt-get update && apt-get install -y cmake clang

COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY sui-execution sui-execution
COPY narwhal narwhal
COPY external-crates external-crates

RUN cargo build --profile ${PROFILE} --bin narwhal-node
RUN cargo build --profile ${PROFILE} --features=benchmark --bin narwhal-benchmark-client

# Production Image
FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y libjemalloc-dev
ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so
ARG PROFILE=release
WORKDIR "$WORKDIR/sui"

# Copy the Narwhal node binary to bin folder
COPY --from=builder sui/target/${PROFILE}/narwhal-node bin/

# This is used for testing a cluster by generating load.
COPY --from=builder sui/target/${PROFILE}/narwhal-benchmark-client bin/

# Copy the entry point file
COPY narwhal/Docker/entry.sh ./

# Now add the entry point
CMD ./entry.sh

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
