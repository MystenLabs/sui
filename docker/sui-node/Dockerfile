# syntax=docker/dockerfile:1
# Chef base: shared rust + cargo-chef install
FROM rust:1.92-bullseye AS chef
ARG PROFILE=release
WORKDIR /sui
RUN apt-get update && apt-get install -y cmake clang
RUN cargo install cargo-chef --locked --version 0.1.71

# Planner stage: produce a deterministic recipe of all dependencies.
# Always cache-misses on source change; cheap (~10s) and runs only to
# compute the recipe. Recipe content is determined by Cargo.toml +
# Cargo.lock + workspace structure.
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates
RUN cargo chef prepare --recipe-path recipe.json

# Deps stage: compile third-party dependencies from the recipe alone.
# Cache key is the content of recipe.json; identical recipes across
# commits → cache hit on the cargo build below, which is the expensive
# step (~12-15 min cold).
FROM chef AS deps
COPY --from=planner /sui/recipe.json recipe.json
RUN cargo chef cook --profile ${PROFILE} --recipe-path recipe.json --bin sui-node

# Builder stage: compile workspace crates against pre-compiled deps.
FROM deps AS builder
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

RUN cargo build --profile ${PROFILE} --bin sui-node

# Production Image
FROM debian:bullseye-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates curl
ARG PROFILE=release
WORKDIR "$WORKDIR/sui"
COPY --from=builder /sui/target/release/sui-node /opt/sui/bin/sui-node
COPY --from=builder /sui/target/release/sui-node /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
