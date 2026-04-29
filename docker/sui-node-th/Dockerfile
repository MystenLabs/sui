# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.92-bullseye AS builder
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

ENV USE_TIDEHUNTER=1
RUN cargo build --profile ${PROFILE} --features typed-store/tidehunter \
    --bin sui-node --bin sui --bin sui-tool --bin stress \
    --target-dir target/tidehunter

# Production Image
FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y ca-certificates curl
ARG PROFILE=release
WORKDIR "$WORKDIR/sui"
# Tidehunter-feature binaries ship at suffixed paths only — no canonical
# aliases, since the storage backend is incompatible with rocksdb and a
# silent swap at /opt/sui/bin/sui-node could corrupt unrelated databases.
COPY --from=builder /sui/target/tidehunter/release/sui-node /opt/sui/bin/sui-node-th
COPY --from=builder /sui/target/tidehunter/release/sui      /opt/sui/bin/sui-th
COPY --from=builder /sui/target/tidehunter/release/sui-tool /opt/sui/bin/sui-tool-th
COPY --from=builder /sui/target/tidehunter/release/stress   /opt/sui/bin/stress-th

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
