# Deps stage: compile third-party dependencies against stubbed workspace
# sources. Caches across commits as long as Cargo.toml/Cargo.lock are
# unchanged, since stubs of *.rs files are deterministic from manifests.
FROM rust:1.92-bullseye AS deps
ARG PROFILE=release
WORKDIR /sui
RUN apt-get update && apt-get install -y cmake clang

COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

# Replace every workspace .rs file with an empty stub so cargo can compile
# third-party deps without any source dependency on workspace members.
# Preserves build.rs (cargo needs them to compile build scripts) and Move
# language source under move-stdlib (referenced via include_bytes!).
RUN set -eu; \
    find . \
      \( -path ./target -o -path ./external-crates/move/crates/move-stdlib/sources \) -prune \
      -o \( -name "*.rs" -a \! -name "build.rs" \) -print0 \
    | xargs -0 -r -n 200 sh -c 'for f in "$@"; do \
        case "$(basename "$f")" in \
          main.rs) echo "fn main(){}" > "$f" ;; \
          *)       : > "$f" ;; \
        esac; \
      done' _

# Pre-compile dependency graph against stubs. `|| true` because some
# workspace members may fail to stub-compile (e.g., type re-exports, macros
# that expand to references the stub doesn't have); third-party deps
# compile FIRST in topological order, so they end up cached in target/
# before any workspace stub fails.
RUN cargo build --profile ${PROFILE} --bin sui-node || true

# Builder stage: replace stubs with real source, then rebuild. Third-party
# deps are already compiled in target/ from the deps stage; cargo skips
# them and only recompiles workspace crates.
FROM deps AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION

COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

# Cargo's mtime-based fingerprint can miss updates when COPY preserves
# uniform timestamps from the Git index. Touch every workspace .rs to
# force fingerprint invalidation against the cached stub artifacts.
RUN find consensus crates sui-execution external-crates -name "*.rs" -exec touch {} +

RUN cargo build --profile ${PROFILE} --bin sui-node

# Production Image
FROM debian:bullseye-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates curl
ARG PROFILE=release
WORKDIR "$WORKDIR/sui"
COPY --from=builder /sui/target/release/sui-node /opt/sui/bin/sui-node
COPY --from=builder /sui/target/release/sui-node /usr/local/bin

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
