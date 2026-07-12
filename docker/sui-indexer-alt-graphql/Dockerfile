# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
# rust:1.92-bullseye
ARG PROFILE=release-unwind
FROM rust:1.92-bullseye AS builder
ARG PROFILE
ARG FEATURES=""
WORKDIR "$WORKDIR/sui"

# sui-indexer need ca-certificates
RUN apt update && apt install -y ca-certificates postgresql

RUN apt-get update && apt-get install -y cmake clang

# COPYs ordered least- to most-frequently-changing for deeper layer cache hits.
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY external-crates external-crates
COPY sui-execution sui-execution
COPY consensus consensus
COPY crates crates

# GIT_REVISION declared late so SHA changes don't invalidate the COPYs above.
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
RUN if [ -n "$FEATURES" ]; then \
        cargo build --locked --profile ${PROFILE} --bin sui-indexer-alt-graphql --features "$FEATURES"; \
    else \
        cargo build --locked --profile ${PROFILE} --bin sui-indexer-alt-graphql; \
    fi

# Production Image
# debian:bullseye-slim
FROM debian@sha256:fdd75562fdcde1039c2480a1ea1cd2cf03b18b6e4cb551cabb03bde66ade8a5d AS runtime
ARG PROFILE
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y libjemalloc-dev ca-certificates curl
ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so
WORKDIR "$WORKDIR/sui"
COPY --from=builder /sui/target/${PROFILE}/sui-indexer-alt-graphql /usr/local/bin
RUN apt update && apt install -y ca-certificates postgresql

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
