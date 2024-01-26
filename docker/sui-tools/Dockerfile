# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.75-bullseye AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sui"
RUN apt-get update && apt-get install -y cmake clang libpq5 libpq-dev

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY narwhal narwhal
COPY external-crates external-crates

RUN cargo build --profile ${PROFILE} \
    --bin sui-node \
    --bin sui \
    --bin sui-faucet \
    --bin stress \
    --bin sui-cluster-test \
    --bin sui-tool

# Production Image
FROM debian:bullseye-slim AS runtime
WORKDIR "$WORKDIR/sui"

# sui-tool needs libpq at runtime
RUN apt-get update && apt-get install -y libpq5 libpq-dev

COPY --from=builder /sui/target/release/sui-node /usr/local/bin
COPY --from=builder /sui/target/release/sui /usr/local/bin
COPY --from=builder /sui/target/release/sui-faucet /usr/local/bin
COPY --from=builder /sui/target/release/stress /usr/local/bin
COPY --from=builder /sui/target/release/sui-cluster-test /usr/local/bin
COPY --from=builder /sui/target/release/sui-tool /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
