FROM rust:1.92-bullseye AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake clang libpq5 libpq-dev \
 && rm -rf /var/lib/apt/lists/*
ARG PROFILE=release
WORKDIR /sui

# COPYs ordered least- to most-frequently-changing for deeper layer cache hits.
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY external-crates external-crates
COPY sui-execution sui-execution
COPY consensus consensus
COPY crates crates

# stress binary doesn't use GIT_REVISION at compile time, so we deliberately
# don't declare ARG/ENV GIT_REVISION in the builder — keeps the cargo-build
# layer cache key stable across commits when source content is unchanged.
RUN cargo build --locked --profile ${PROFILE} --bin stress

# Production Image
#
# Mirrors the docker/sui-node and docker/sui-tools pattern: slim base + only
# the runtime tools consumers actually invoke. Previously this stage used
# `rust:1.92-bullseye` plus a `git clone` of the full repo, producing a
# ~1.2 GB compressed image (vs sui-node's ~114 MB). The oversized export
# made BuildKit's `#28 exporting to client directory` step prone to gRPC
# transport hangs under PVC/network contention.
FROM debian:bullseye-slim AS runtime
# Runtime needs:
#   - sui-network-aux mounts its own entry.sh and runs `aws s3 cp` to fetch
#     genesis + keystore, with a `curl` fallback.
#   - sui-antithesis uses the in-image entrypoint below (curl + bash only).
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl awscli \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /sui
ARG PROFILE=release
COPY --from=builder /sui/target/${PROFILE}/stress /usr/local/bin/stress
COPY docker/stress/entrypoint.sh /docker/stress/entrypoint.sh
RUN chmod +x /docker/stress/entrypoint.sh

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION

CMD ["/docker/stress/entrypoint.sh"]
