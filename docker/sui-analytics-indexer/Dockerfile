FROM rust:1.85-bullseye as build

ARG PROFILE=release
WORKDIR /work

RUN apt-get update && apt-get install -y cmake clang

COPY .git/ .git/
COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

# Enable debug symbols even in release profile
ENV RUSTFLAGS="-C debuginfo=2"

RUN cargo build --profile ${PROFILE} --bin sui-analytics-indexer

# --------------------
# Runtime Stage
# --------------------
FROM debian:12-slim as runtime

# Install heaptrack, jemalloc, and basic tooling
RUN apt-get update && apt-get install -y --no-install-recommends \
  ca-certificates \
  curl \
  libjemalloc-dev \
  heaptrack \
  procps \
  && apt-get clean \
  && rm -rf /var/lib/apt/lists/*

# Optional: use jemalloc for consistent memory profiling
ENV LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libjemalloc.so

# Copy binary with debug symbols
COPY --from=build --chmod=755 /work/target/release/sui-analytics-indexer /opt/sui/bin/sui-analytics-indexer

# Optional: run under heaptrack by default
# You can comment this out and use Pulumi to override the command if you prefer
# CMD ["sh", "-c", "ulimit -c unlimited && heaptrack /opt/sui/bin/sui-analytics-indexer /config/analytics-indexer.yaml"]

