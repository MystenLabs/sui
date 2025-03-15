# Build application
FROM rust:1.85-bullseye AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sui"

# sui-indexer needs postgres libpq5 and ca-certificates
RUN apt update && apt install -y libpq5 ca-certificates libpq-dev postgresql

RUN apt-get update && apt-get install -y cmake clang

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

RUN cargo build --profile ${PROFILE} --bin sui-indexer

# Production Image with Debugging Tools
FROM debian:bullseye-slim AS runtime
# Install jemalloc and debugging utilities
RUN apt-get update && apt-get install -y \
  libjemalloc-dev \
  ca-certificates \
  curl \
  bash \
  coreutils \
  wget \
  vim \
  procps \
  net-tools \
  iputils-ping \
  less \
  libpq5 \
  libpq-dev \
  postgresql \
  && rm -rf /var/lib/apt/lists/*

ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so
WORKDIR "$WORKDIR/sui"
COPY --from=builder /sui/target/release/sui-indexer /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION

# Default to bash shell for debugging purposes
CMD ["/bin/bash"]

