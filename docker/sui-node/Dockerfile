# Build application
#
# Multi-stage with cargo-chef: the deps layer caches on Cargo.lock changes
# only, so source-only commits reuse compiled deps. No BuildKit-only
# directives — works on plain Docker.
#
FROM rust:1.92-bullseye AS chef
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake clang \
 && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked --version 0.1.71
WORKDIR /sui

FROM chef AS planner
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates
RUN cargo chef prepare --recipe-path recipe.json --bin sui-node

FROM chef AS deps
ARG PROFILE=release
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY --from=planner /sui/recipe.json recipe.json
# external-crates/move is a separate workspace; cargo-chef needs its source to
# resolve and build path deps that sui-node pulls from there.
COPY external-crates external-crates
RUN cargo chef cook --locked --profile ${PROFILE} --recipe-path recipe.json --bin sui-node

FROM deps AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates
# Force cargo to recompile local crates: stub mtimes from the cook stage can
# otherwise look "newer" than real source, and cargo skips rebuilds.
RUN find consensus crates sui-execution external-crates -name "*.rs" -exec touch {} +
RUN cargo build --locked --profile ${PROFILE} --bin sui-node

# Production Image
FROM debian:bullseye-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /sui
ARG PROFILE=release
COPY --from=builder /sui/target/${PROFILE}/sui-node /opt/sui/bin/sui-node
COPY --from=builder /sui/target/${PROFILE}/sui-node /usr/local/bin/sui-node

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
