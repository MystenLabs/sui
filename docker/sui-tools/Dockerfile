# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.81-bullseye AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sui"
RUN apt-get update && apt-get install -y cmake clang libpq5 libpq-dev

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

RUN cargo build --profile ${PROFILE} \
    --bin stress \
    --bin sui-faucet \
    --bin sui-bridge \
    --bin bridge-indexer \
    --bin deepbook-indexer \
    --bin sui-bridge-cli \
    --bin sui \
    --bin sui-cluster-test \
    --bin sui-tool

# Production Image
FROM debian:bullseye-slim AS runtime
WORKDIR "$WORKDIR/sui"

# sui-tool needs libpq at runtime and uses git when fetching move dependencies during build
RUN apt-get update && apt-get install -y libpq5 libpq-dev ca-certificates git

COPY --from=builder /sui/target/release/stress /usr/local/bin
COPY --from=builder /sui/target/release/sui-faucet /usr/local/bin
COPY --from=builder /sui/target/release/sui-bridge /usr/local/bin
COPY --from=builder /sui/target/release/bridge-indexer /usr/local/bin
COPY --from=builder /sui/target/release/deepbook-indexer /usr/local/bin
COPY --from=builder /sui/target/release/sui-bridge-cli /usr/local/bin
COPY --from=builder /sui/target/release/sui /usr/local/bin
COPY --from=builder /sui/target/release/sui-cluster-test /usr/local/bin
COPY --from=builder /sui/target/release/sui-tool /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
