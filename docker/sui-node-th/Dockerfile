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
RUN cargo build --profile ${PROFILE} --features typed-store/tidehunter --bin sui-node --target-dir target/tidehunter

# Production Image
FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y ca-certificates curl
ARG PROFILE=release
WORKDIR "$WORKDIR/sui"
# Only ship the tidehunter binary at a suffixed path. Intentionally no
# /opt/sui/bin/sui-node or /usr/local/bin/sui-node aliases — the binary
# uses the tidehunter storage backend, which is incompatible with rocksdb,
# and silently swapping at the canonical paths would let callers run it
# as plain sui-node and corrupt unrelated databases. K8s pod entrypoints
# must call /opt/sui/bin/sui-node-th explicitly. mp3's binary_upload_config
# extracts this path to s3://sui-releases/{sha}/sui-node-th via {basename}
# substitution; that S3 location is the contract for ansible bare-metal
# nodes (the build_with_tidehunter selector in
# sui-operations/ansible/playbook/sui/roles/sui-node-{internal,external}).
COPY --from=builder /sui/target/tidehunter/release/sui-node /opt/sui/bin/sui-node-th

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
