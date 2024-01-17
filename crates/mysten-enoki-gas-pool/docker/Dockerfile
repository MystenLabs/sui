FROM rust:1.73-bullseye  AS chef
WORKDIR sui
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
RUN apt-get update && apt-get install -y cmake clang

# Build and cache all dependencies.
FROM chef AS builder
WORKDIR /
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

# Production Image
FROM debian:bullseye-slim AS runtime
RUN apt-get update && apt-get install -y libjemalloc-dev ca-certificates
COPY --from=builder /target/release/sui-gas-station /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION

