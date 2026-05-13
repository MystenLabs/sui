FROM rust:1.92-bullseye AS build

ARG PROFILE=release
WORKDIR /work

RUN apt-get update && apt-get install -y cmake clang

# COPYs ordered least- to most-frequently-changing for deeper layer cache hits.
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY external-crates external-crates
COPY sui-execution sui-execution
COPY consensus consensus
COPY crates crates

# .git/ copied late so per-commit metadata churn doesn't invalidate the COPYs above.
COPY .git/ .git/
RUN cargo build --locked --profile ${PROFILE} --bin sui-kv-rpc

FROM gcr.io/distroless/cc-debian12 as deploy

COPY --from=build --chmod=755 /work/target/release/sui-kv-rpc /opt/sui/bin/sui-kv-rpc
