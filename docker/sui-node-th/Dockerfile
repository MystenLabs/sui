# Build application
#
# Single-stage builder + minimal runtime. Layer cache short-circuits when the
# COPY inputs are byte-identical to a previous build.
#
FROM rust:1.92-bullseye AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake clang \
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

# GIT_REVISION declared late so SHA changes don't invalidate the COPYs above.
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
ENV USE_TIDEHUNTER=1
RUN cargo build --locked --profile ${PROFILE} \
    --features typed-store/tidehunter,sui-tool/tideconsole \
    --bin sui-node --bin sui --bin sui-tool --bin stress

# Production Image
FROM debian:bullseye-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /sui
# Tidehunter-feature binaries ship at suffixed paths only — no canonical
# aliases, since the storage backend is incompatible with rocksdb and a
# silent swap at /opt/sui/bin/sui-node could corrupt unrelated databases.
# Source path is hardcoded to /release/ because the release and bench
# profiles both emit there (bench inherits release's dir-name).
COPY --from=builder /sui/target/release/sui-node /opt/sui/bin/sui-node-th
COPY --from=builder /sui/target/release/sui      /opt/sui/bin/sui-th
COPY --from=builder /sui/target/release/sui-tool /opt/sui/bin/sui-tool-th
COPY --from=builder /sui/target/release/stress   /opt/sui/bin/stress-th

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
