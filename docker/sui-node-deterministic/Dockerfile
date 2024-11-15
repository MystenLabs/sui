ARG PROFILE=release
ARG BUILD_DATE
ARG GIT_REVISION
# ARG RUST_VERSION=1.76.0

FROM stagex/busybox:sx2024.11.0@sha256:3d128909dbc8e7b6c4b8c3c31f4583f01a307907ea179934bb42c4ef056c7efd AS busybox
FROM stagex/musl:sx2024.11.0@sha256:d7f6c365f5724c65cadb2b96d9f594e46132ceb366174c89dbf7554897f2bc53 AS musl
FROM stagex/rust:sx2024.11.0@sha256:f23fa04c29ab0250b39c38ee1cc4394a1ea3ec91b865070716a585d2b96389ed AS rust
FROM stagex/gcc:sx2024.11.0@sha256:49ea63c81c65f8be25c242b7e64f2758b23effdaafb458b5862d0f23ec803075 AS gcc
FROM stagex/llvm:sx2024.11.0@sha256:27da8a38ec621317dbafbf1dbdefb07a5b007d1d28ae86393480209652ed3770 AS llvm
FROM stagex/libunwind:sx2024.11.0@sha256:290b8d183a467edc55e338471632f2e40859aef92a4eecf12415ca29b9c16e9f AS libunwind
FROM stagex/openssl:sx2024.11.0@sha256:8e3eb24b4d21639f7ea204b89211d8bc03a2e1b729fb1123f8d0b3752b4beaa1 AS openssl
FROM stagex/zlib:sx2024.11.0@sha256:09d63654e27decb6147f1b23005d539e30db8e53eb2d284e824bcf4d4e7c3c11 AS zlib
FROM stagex/ca-certificates:sx2024.11.0@sha256:a84695f983a448a82acfe78af11f33c6a66b27124266e1fdc3ecfb8dc5852573 AS ca-certificates

FROM stagex/binutils:sx2024.11.0@sha256:eff721a796fdfba8c34e21a487b0e376fb55ca2633524926998f1660fbb810de AS binutils
FROM stagex/make:sx2024.11.0@sha256:ad81793d21d9778421925085c52734fdcca443957ade4f9bb80e4032b88252db AS make
FROM stagex/clang:sx2024.11.0@sha256:c26069d378f36c06b5d91e3aba907521ec79eb0864d65a4c28a2db17947ec25f AS clang
FROM stagex/linux-headers:sx2024.11.0@sha256:bafd40b92b6333575aface5aa48820c18c14b0bce02c8ef91f1814b6234652a7 AS linux-headers

FROM stagex/cross-x86_64-gnu-gcc:sx2024.11.0@sha256:cca4ab2ea51adb4797bf50aa03cc855ba992e9ae37840d1873fffbd773fea5d7 AS cross-x86_64-gnu-gcc
FROM stagex/cross-x86_64-gnu-rust:sx2024.11.0@sha256:2dbfbd1cf7d034450c197582aeb90dd12a68cf6dfdd10e2abfda1ff202a75de4 AS cross-x86_64-gnu-rust
FROM stagex/glibc:sx2024.11.0@sha256:5e4f3c0b0b811dac9bfa96c43215856de4a7ec4a7866bc019608b0889f78633a AS glibc

FROM scratch AS base

FROM base AS fetch

COPY --from=busybox . /
COPY --from=musl . /
COPY --from=rust . /

COPY --from=gcc . /
COPY --from=llvm . /
COPY --from=libunwind . /
COPY --from=openssl . /
COPY --from=zlib . /

# NOTE: Necessary for `cargo fetch`, but CA trust is not relied upon
COPY --from=ca-certificates . /

COPY . /sui

WORKDIR sui

RUN cargo fetch

FROM fetch AS build

# Rust build deps

COPY --from=binutils . /
COPY --from=gcc . /
COPY --from=llvm . /
COPY --from=make . /
COPY --from=musl . /

# Sui build deps

COPY --from=clang . /
COPY --from=linux-headers . /

COPY --from=cross-x86_64-gnu-gcc . /
COPY --from=cross-x86_64-gnu-rust . /
COPY --from=glibc . /

ARG PROFILE
ARG GIT_REVISION

ENV RUST_BACKTRACE=1
ENV RUSTFLAGS="-C codegen-units=1 -C target-feature=+crt-static -C linker=/usr/bin/x86_64-linux-gnu-gcc"
ENV GIT_REVISION=${GIT_REVISION}
ENV PROFILE=${PROFILE}

RUN --network=none cargo build --target x86_64-unknown-linux-gnu --frozen --profile ${PROFILE} --bin sui-node

FROM scratch AS install

COPY --from=busybox . /

COPY --from=busybox . /rootfs
COPY --from=libunwind . /rootfs
COPY --from=gcc . /rootfs

# support current + legacy paths
RUN mkdir -p /rootfs/opt/sui/bin
RUN mkdir -p /rootfs/usr/local/bin
COPY --from=build sui/target/x86_64-unknown-linux-gnu/release/sui-node /rootfs/opt/sui/bin/sui-node


RUN --network=none find /rootfs -exec touch -hcd "@0" "{}" +

FROM scratch AS package

ARG PROFILE
ARG GIT_REVISION

LABEL build-date=${BUILD_DATE}
LABEL git-revision=${GIT_REVISION}

COPY --from=install /rootfs /

RUN ln -s /opt/sui/bin/sui-node /usr/local/bin/sui-node

