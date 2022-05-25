ARG BUILD_MODE="release"
FROM lukemathwalker/cargo-chef:latest-rust-1.60.0-slim as builder

# Dictates whether the underlying node should be built as a dev or
# production binary. If empty, then the development mode will be built.
# If passed as BUILD_MODE=--release, then the production will be built.
ARG BUILD_MODE

RUN echo "Will build image with mode: ${BUILD_MODE}"

# Install basic dependencies
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC apt-get install -y --no-install-recommends \
    tzdata \
    git \
    ca-certificates \
    curl \
    build-essential \
    libssl-dev \
    pkg-config \
    clang \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Install the fmt
RUN rustup component add rustfmt

# Set the workdir to the build folder
WORKDIR "$WORKDIR/build/narwhal"

# Copy all the files in the workdir excluding everything
# from the .dockerignore
COPY . .

# Build the binary named "node"
RUN cargo build --${BUILD_MODE} --bin node

# Creat another layer so we can re-use caching
FROM rust:1.60-slim-bullseye
ARG BUILD_MODE

WORKDIR "$WORKDIR/build"

RUN apt-get update \
    && apt-get install -y --no-install-recommends libssl1.1 iputils-ping telnet curl \
    && rm -rf /var/lib/apt/lists/*

# Copy the Narwhal node binary to bin folder
COPY --from=builder /build/narwhal/target/${BUILD_MODE}/node bin/

# Copy the entry point file
COPY Docker/entry.sh ./

# Now add the entry point
CMD ./entry.sh
