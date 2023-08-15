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

RUN cargo build --profile ${PROFILE} --bin sui-indexer

# Production Image
FROM debian:bullseye-slim AS runtime
WORKDIR "$WORKDIR/sui"
COPY --from=builder /sui/target/release/sui-indexer /usr/local/bin
# sui-indexer needs postgres libpq5 and ca-certificates
RUN apt update && apt install -y libpq5 ca-certificates

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
