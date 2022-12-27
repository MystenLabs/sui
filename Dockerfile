FROM rust:1.65.0 AS chef
WORKDIR sui
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    apt-get install -y cmake clang
RUN apt-get install -y strace
# Plan out the 3rd-party dependencies that need to be built.
#
# This is done by:
#   1. Copy in Cargo.toml, Cargo.lock, and the workspace-hack crate
#   2. Removing all workspace crates, other than the workpsace-hack
#      crate, from the workspace Cargo.toml file.
#   3. Update the lockfile in order to reflect the changes to the
#      root Cargo.toml file.
FROM chef AS planner

RUN cargo install cargo-nextest --locked

COPY . ./
ENV PATH="$PATH:/root/.cargo/bin"
ENV CARGO_HOME=/build/.cargo
ENV CARGO_TARGET_DIR=/build/target
RUN mkdir -p /root/.cargo/bin
RUN ./scripts/simtest/install.sh
# RUN cargo simtest build
