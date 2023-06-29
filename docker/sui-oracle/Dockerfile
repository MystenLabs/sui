FROM rust:1.70.0 AS chef
WORKDIR /sui
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
RUN apt-get update && apt-get install -y cmake clang

FROM chef AS builder 

# Build application
#
# Copy in the rest of the crates (and an unmodified Cargo.toml and Cargo.lock)
# and build the application. At this point no dependencies should need to be
# built as they were built and cached by the previous layer.
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY sui-execution sui-execution
COPY narwhal narwhal
COPY external-crates external-crates
RUN cargo build \
    --bin sui-oracle

# Production Image
FROM debian:bullseye-slim AS runtime
WORKDIR /sui
COPY --from=builder /sui/target/debug/sui-oracle /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION

