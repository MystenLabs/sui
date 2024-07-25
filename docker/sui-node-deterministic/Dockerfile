ARG PROFILE=release
ARG BUILD_DATE
ARG GIT_REVISION
# ARG RUST_VERSION=1.76.0

FROM stagex/busybox:sx2024.04.2@sha256:8cb9360041cd17e8df33c5cbc6c223875045c0c249254367ed7e0eb445720757 AS busybox
FROM stagex/musl:sx2024.04.2@sha256:f888fcf45fabaaae3d0268bcec902ceb94edba7bf8d09ef6966ebb20e00b7127 AS musl
FROM stagex/rust:sx2024.04.2@sha256:e7882823078d49da4302578526dc35d6b2afad7507740c2f9bcd406a79befbc9 AS rust
FROM stagex/gcc:sx2024.04.2@sha256:c67989de74d82eddeaf0d458edb1ca35b88064d3a66d5631c3530d5f10975f5e AS gcc
FROM stagex/llvm:sx2024.04.2@sha256:ae430dbdd1a6d546bb8aef817d09d3fb4e0145f81c071647666b7a9f7b69f8a1 AS llvm
FROM stagex/libunwind:sx2024.04.2@sha256:622bbc0bcd502d349624fe90bd3cfc63595a71d450702d4e746abf8918351e2b AS libunwind
FROM stagex/openssl:sx2024.04.2@sha256:e719432f169a5ff606f52004a554e58780335f9635b1cf271d002ca7b1d1a206 AS openssl
FROM stagex/zlib:sx2024.04.2@sha256:d5b923b8f1b0382b8bdde96f36c0b8b9f694c97b14a9071bd96f2ffc46124e03 AS zlib
FROM stagex/ca-certificates:sx2024.04.2@sha256:f9fe6e67df91083fee3d88cf221f84ef77f0b67480fb5b0689e890509a712533 AS ca-certificates

FROM stagex/binutils:sx2024.04.2@sha256:311f8c2bd2b586bf7210c40dde43d0e0d5e76af4e1e688ad129f945691e3e105 AS binutils
FROM stagex/make:sx2024.04.2@sha256:8357ff7a8afa260ae3cc8e8993d80bce524d9802b2033020f7ea7f8f85133634 AS make
FROM stagex/clang:sx2024.04.2@sha256:489d7f0b8694ecb4f21af80f4fee4908a4fabd92740af3648ee3715212da409e AS clang
FROM stagex/linux-headers:sx2024.04.2@sha256:fe366787ecaf36393b17ede6108161af4136bf5b7521e49f0a005a6ef68ef8db AS linux-headers

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

ARG PROFILE
ARG GIT_REVISION

ENV RUST_BACKTRACE=1
ENV RUSTFLAGS='-C target-feature=+crt-static -C codegen-units=1'
ENV GIT_REVISION=${GIT_REVISION}
ENV PROFILE=${PROFILE}

RUN --network=none cargo build --target x86_64-unknown-linux-musl --frozen --profile ${PROFILE} --bin sui-node

FROM scratch AS install

COPY --from=busybox . /

COPY --from=busybox . /rootfs
COPY --from=libunwind . /rootfs
COPY --from=gcc . /rootfs
COPY --from=musl . /rootfs

# support current + legacy paths
RUN mkdir -p /rootfs/opt/sui/bin
RUN mkdir -p /rootfs/usr/local/bin
COPY --from=build sui/target/x86_64-unknown-linux-musl/release/sui-node /rootfs/opt/sui/bin/sui-node


RUN --network=none find /rootfs -exec touch -hcd "@0" "{}" +

FROM scratch AS package

ARG PROFILE
ARG GIT_REVISION

LABEL build-date=${BUILD_DATE}
LABEL git-revision=${GIT_REVISION}

COPY --from=install /rootfs /

RUN ln -s /opt/sui/bin/sui-node /usr/local/bin/sui-node

