# syntax=docker/dockerfile:1.4

# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.92-bullseye AS builder
ARG PROFILE=release
ARG GIT_REVISION
ARG TARGETARCH
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sui"
RUN apt-get update && apt-get install -y cmake clang

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

# BuildKit cache mounts persist target/ and the cargo registry/git caches
# across builds on the same builder. Cache ids are split by arch so amd64
# and arm64 builds don't stomp each other. sharing=locked serializes
# concurrent builds for the same cache id (cargo's target/ is not safe
# under concurrent writers).
#
# After the build, copy the binary out of the cache mount so the next stage
# can COPY --from=builder. Cache mount contents do not persist into the
# image layer.
RUN --mount=type=cache,target=/sui/target,id=sui-node-target-${TARGETARCH},sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry-${TARGETARCH},sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git-${TARGETARCH},sharing=locked \
    cargo build --profile ${PROFILE} --bin sui-node \
    && mkdir -p /opt/sui/bin \
    && cp /sui/target/release/sui-node /opt/sui/bin/sui-node

# Production Image
FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y ca-certificates curl
ARG PROFILE=release
WORKDIR "$WORKDIR/sui"
# Both bench and release profiles copy from release dir
COPY --from=builder /opt/sui/bin/sui-node /opt/sui/bin/sui-node
# Support legacy usages of /usr/local/bin/sui-node
COPY --from=builder /opt/sui/bin/sui-node /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
