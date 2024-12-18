FROM rust:1.81-bullseye  AS chef
WORKDIR sui
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
RUN apt-get update && apt-get install -y cmake clang

FROM chef AS builder 

# Build application
COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates
RUN cargo build --release \
    --bin sui-source-validation-service

# Production Image
FROM debian:bullseye-slim AS runtime
WORKDIR sui
RUN apt update && apt install -y git
COPY --from=builder /sui/target/release/sui-source-validation-service /usr/local/bin
COPY crates/sui-source-validation-service/config.toml /var/sui/

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
