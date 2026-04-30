# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.92-bullseye AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
# AWS credentials for sccache S3 backend, passed as build args. Used only
# during cargo build below; not persisted into the final runtime image.
ARG AWS_ACCESS_KEY_ID
ARG AWS_SECRET_ACCESS_KEY
WORKDIR "$WORKDIR/sui"
RUN apt-get update && apt-get install -y cmake clang curl

# Install upstream mold 2.40.4 (matches sui CI's setup-sccache action)
RUN set -eux; \
    MOLD_VERSION=2.40.4; \
    arch=$(uname -m); \
    case "$arch" in \
      x86_64)  mold_arch=x86_64-linux;  expected_sha=4c999e19ffa31afa5aa429c679b665d5e2ca5a6b6832ad4b79668e8dcf3d8ec1 ;; \
      aarch64) mold_arch=aarch64-linux; expected_sha=c799b9ccae8728793da2186718fbe53b76400a9da396184fac0c64aa3298ec37 ;; \
      *) echo "unsupported arch: $arch"; exit 1 ;; \
    esac; \
    curl -fsSL "https://github.com/rui314/mold/releases/download/v${MOLD_VERSION}/mold-${MOLD_VERSION}-${mold_arch}.tar.gz" -o /tmp/mold.tgz; \
    echo "${expected_sha}  /tmp/mold.tgz" | sha256sum -c -; \
    tar xz -C /opt -f /tmp/mold.tgz; \
    ln -sf "/opt/mold-${MOLD_VERSION}-${mold_arch}/bin/mold" /usr/local/bin/mold; \
    ln -sf "/opt/mold-${MOLD_VERSION}-${mold_arch}/bin/mold" /usr/local/bin/ld.mold; \
    rm /tmp/mold.tgz; \
    mold --version

# Install sccache 0.10.0 from upstream
RUN set -eux; \
    SCCACHE_VERSION=0.10.0; \
    arch=$(uname -m); \
    case "$arch" in \
      x86_64)  sc_arch=x86_64-unknown-linux-musl ;; \
      aarch64) sc_arch=aarch64-unknown-linux-musl ;; \
      *) echo "unsupported arch: $arch"; exit 1 ;; \
    esac; \
    curl -fsSL "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-${sc_arch}.tar.gz" -o /tmp/sccache.tgz; \
    tar xz -C /tmp -f /tmp/sccache.tgz; \
    install -m 0755 "/tmp/sccache-v${SCCACHE_VERSION}-${sc_arch}/sccache" /usr/local/bin/sccache; \
    rm -rf /tmp/sccache.tgz "/tmp/sccache-v${SCCACHE_VERSION}-${sc_arch}"; \
    sccache --version

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

# Build with mold linker + sccache S3 backend. RUSTFLAGS replicates sui's
# .cargo/config.toml baseline plus -C link-arg=-fuse-ld=mold; keep in sync
# with .github/actions/setup-sccache/action.yml in the sui repo.
RUN AWS_ACCESS_KEY_ID="$AWS_ACCESS_KEY_ID" \
    AWS_SECRET_ACCESS_KEY="$AWS_SECRET_ACCESS_KEY" \
    SCCACHE_BUCKET=mystenlabs-sccache \
    SCCACHE_REGION=us-west-2 \
    SCCACHE_S3_KEY_PREFIX=mp3-sui-node-experiment \
    RUSTC_WRAPPER=sccache \
    RUSTFLAGS="-C force-frame-pointers=yes -C force-unwind-tables=yes -A unknown_lints -A mismatched_lifetime_syntaxes -C link-arg=-fuse-ld=mold" \
    cargo build --profile ${PROFILE} --bin sui-node && \
    sccache --show-stats

# Production Image
FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y ca-certificates curl
ARG PROFILE=release
WORKDIR "$WORKDIR/sui"
# Both bench and release profiles copy from release dir
COPY --from=builder /sui/target/release/sui-node /opt/sui/bin/sui-node
# Support legacy usages of /usr/local/bin/sui-node
COPY --from=builder /sui/target/release/sui-node /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
