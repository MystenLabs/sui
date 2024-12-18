# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.81-bullseye AS builder
ARG PROFILE=release
ENV PROFILE=$PROFILE
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sui"
RUN apt-get update && apt-get install -y cmake clang libpq-dev

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

RUN cargo build --profile ${PROFILE} --bin sui-graphql-rpc --features staging

# Production Image
FROM debian:bullseye-slim AS runtime
WORKDIR "$WORKDIR/sui"
# Both bench and release profiles copy from release dir
RUN mkdir -p /opt/sui/bin
COPY --from=builder /sui/target/release/sui-graphql-rpc /opt/sui/bin
RUN apt-get update && apt-get install -y libpq5 ca-certificates libpq-dev postgresql

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
