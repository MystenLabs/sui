# Build application
#
# Single-stage builder + minimal runtime. Layer cache short-circuits when the
# COPY inputs are byte-identical to a previous build.
#
FROM rust:1.92-bullseye AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake clang libpq5 libpq-dev \
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
RUN cargo build --locked --profile ${PROFILE} \
    --bin sui-faucet \
    --bin sui-bridge \
    --bin bridge-indexer \
    --bin sui-bridge-cli \
    --bin sui \
    --bin sui-cluster-test \
    --bin sui-tool \
    --bin move-analyzer

# Production Image
FROM debian:bullseye-slim AS runtime
# sui-tool needs libpq at runtime and git when fetching Move dependencies.
# openssh-client is explicit — without it `--no-install-recommends` drops the
# git→ssh-client recommendation, breaking git@/ssh:// Move sources.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git openssh-client libpq5 libpq-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /sui
# Source path is hardcoded to /release/ because the bench profile inherits
# release's dir-name.
COPY --from=builder /sui/target/release/sui-faucet       /usr/local/bin/sui-faucet
COPY --from=builder /sui/target/release/sui-bridge       /usr/local/bin/sui-bridge
COPY --from=builder /sui/target/release/bridge-indexer   /usr/local/bin/bridge-indexer
COPY --from=builder /sui/target/release/sui-bridge-cli   /usr/local/bin/sui-bridge-cli
COPY --from=builder /sui/target/release/sui              /usr/local/bin/sui
COPY --from=builder /sui/target/release/sui-cluster-test /usr/local/bin/sui-cluster-test
COPY --from=builder /sui/target/release/sui-tool         /usr/local/bin/sui-tool
COPY --from=builder /sui/target/release/move-analyzer    /usr/local/bin/move-analyzer

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
