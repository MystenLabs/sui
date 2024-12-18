FROM rust:1.81-bullseye  AS chef
WORKDIR sui
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
RUN apt-get update && apt-get install -y cmake clang libpq5 ca-certificates libpq-dev postgresql

# Build application
FROM chef AS builder 

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates
RUN cargo build --release \
    --bin sui-oracle \
    --bin suins-indexer
RUN mkdir /sui/bin/
RUN find /sui/target/release/ -maxdepth 1 -type f -executable -print 
RUN find /sui/target/release/ -maxdepth 1 -type f -executable -print | xargs cp -t /sui/bin/

# Production Image
FROM debian:bullseye-slim AS runtime
WORKDIR sui
COPY --from=builder /sui/bin /tmp/sui-bin
RUN find /tmp/sui-bin -maxdepth 1 -type f -executable -print | xargs cp -t /usr/local/bin/
RUN rm -rf /tmp/sui-bin
RUN apt update && apt install -y libpq5 libpq-dev postgresql

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION

