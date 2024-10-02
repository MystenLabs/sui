ARG PROFILE=release
# ARG BUILD_DATE
# ARG GIT_REVISION
ARG RUST_VERSION=1.81.0

FROM scratch AS base

FROM stagex/rust:${RUST_VERSION} AS rust
FROM base AS fetch

COPY --from=stagex/busybox . /
COPY --from=stagex/musl . /
COPY --from=rust . /

COPY --from=stagex/gcc . /
COPY --from=stagex/llvm . /
COPY --from=stagex/libunwind . /
COPY --from=stagex/openssl . /
COPY --from=stagex/zlib . /

# NOTE: Necessary for `cargo fetch`, but CA trust is not relied upon
COPY --from=stagex/ca-certificates . /

# HACK: gcc puts things in /usr/lib64 
COPY --from=stagex/gcc /usr/lib64/* /usr/lib/

RUN cargo new canary

WORKDIR canary

RUN cargo fetch

FROM fetch AS build

# Rust build deps

COPY --from=stagex/binutils . /
COPY --from=stagex/gcc . /
COPY --from=stagex/llvm . /
COPY --from=stagex/make . /
COPY --from=stagex/musl . /

# Sui build deps

COPY --from=stagex/clang . /
COPY --from=stagex/linux-headers . /

ARG PROFILE
# ARG GIT_REVISION

ENV RUST_BACKTRACE=1
ENV RUSTFLAGS='-C target-feature=-crt-static -C codegen-units=1'
# ENV GIT_REVISION=${GIT_REVISION}
ENV PROFILE=${PROFILE}

RUN --network=none cargo build --frozen --profile ${PROFILE}

FROM scratch AS install

COPY --from=stagex/busybox . /

COPY --from=stagex/busybox . /rootfs
COPY --from=stagex/libunwind . /rootfs
COPY --from=stagex/gcc . /rootfs
COPY --from=stagex/musl . /rootfs

# HACK: In the current release of stagex, gcc puts things in /usr/lib64,
# but we expect them in /usr/lib
COPY --from=stagex/gcc /usr/lib64/* /rootfs/usr/lib/

RUN mkdir -p /rootfs/usr/local/bin
COPY --from=build canary/target/release/canary /rootfs/usr/local/bin/canary

RUN --network=none find /rootfs -exec touch -hcd "@0" "{}" +

FROM scratch AS package

# ARG GIT_REVISION

# LABEL build-date=${BUILD_DATE}
# LABEL git-revision=${GIT_REVISION}

COPY --from=install /rootfs /
