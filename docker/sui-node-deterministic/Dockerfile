# This builds successfully against sui commit e192635917f584c0704e908e7441c1d88c65fa25
ARG PROFILE
ARG CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu

FROM stagex/pallet-rust@sha256:9c38bf1066dd9ad1b6a6b584974dd798c2bf798985bf82e58024fbe0515592ca AS pallet-rust
FROM stagex/core-cross-x86_64-gnu-gcc@sha256:9b9f69f5a9a15c01fe33461c8d927bcfb03340d8b76019a54a92a95dfb200b62 AS cross-x86_64-gnu-gcc
FROM stagex/core-cross-x86_64-gnu-rust@sha256:121be2717276879ecd0a52ebe663f8975eba6fb07660b49f401104ec45d5a8ab AS cross-x86_64-gnu-rust
FROM stagex/core-cross-x86_64-gnu-libunwind@sha256:da98f6a5d59c32357e29d9c516d55152ada77db57ec36ea0ccd4a33c73127b36 AS cross-x86_64-gnu-libunwind
FROM stagex/core-clang@sha256:8f9293b4d06bb1045f9b8ace88f2eb803ba07a9428d04dfadf19690b33f5a644 AS clang
FROM stagex/core-llvm@sha256:583ecda677f51b69857f8027dfc58f4a931d1adc4d16214870a373505210d973 AS llvm
FROM stagex/core-lld@sha256:78de63b617743841d4ee0f3a44627399566d5eb3432820a80f994a3e9deba586 AS lld
FROM stagex/core-libffi@sha256:64d087343541401271cf9fec6b7bd788040c72a16918748ae36c171e53e94002 AS libffi
FROM stagex/core-make@sha256:6134672bb23defb919ad11fc432d82b378cd543c83badf081d4990b50df64bf7 AS make
FROM stagex/user-glibc@sha256:287280459258953d86a8e42b87003d2b0b1ddf0aa2e0fa70ba6e33f275b45901 AS glibc

################
# target=build #
################

FROM pallet-rust AS build
ARG PROFILE=release
ARG CARGO_BUILD_TARGET
ARG GIT_REVISION

COPY --from=cross-x86_64-gnu-gcc . /
COPY --from=cross-x86_64-gnu-rust . /
COPY --from=cross-x86_64-gnu-libunwind . /
COPY --from=clang . /
COPY --from=llvm . /
COPY --from=lld . /
COPY --from=libffi . /
COPY --from=make . /
COPY --from=glibc . /

# Basic Rust settings
ENV RUST_BACKTRACE=full
ENV RUSTFLAGS="-C codegen-units=1"

# Cross-compilation glue for C deps like jemalloc. This forces C/C++ compilation
# and linking through the cross glibc toolchain using env vars instead of only RUSTFLAGS.
ENV CC=/opt/cross/bin/x86_64-linux-gnu-gcc
ENV CXX=/opt/cross/bin/x86_64-linux-gnu-g++
ENV AR=/opt/cross/bin/x86_64-linux-gnu-ar

# Tell Cargo/rustc which linker to use for the target triple.
# Must match CARGO_BUILD_TARGET (default x86_64-unknown-linux-gnu).
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/opt/cross/bin/x86_64-linux-gnu-gcc

# Helpful for other C deps that use pkg-config while cross-compiling
ENV PKG_CONFIG_ALLOW_CROSS=1

WORKDIR /sui
COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates

RUN cargo build --profile ${PROFILE} --target ${CARGO_BUILD_TARGET} --bin sui-node

##################
# target=package #
##################

FROM scratch AS package
ARG PROFILE=release
ARG CARGO_BUILD_TARGET
WORKDIR /sui
ENV LD_LIBRARY_PATH=/lib64

COPY --from=build /opt/cross/x86_64-linux-gnu/lib64/ /lib64/
COPY --from=build /opt/cross/x86_64-linux-gnu/lib/ /lib64/

# NOTE: jemallocator usually links jemalloc statically via jemalloc-sys.
# If you ever see "error while loading shared libraries: libjemalloc.so.2",
# you'll also need to COPY that .so into /lib64 here.

# Copy the binary
COPY --from=build /sui/target/${CARGO_BUILD_TARGET}/${PROFILE}/sui-node /opt/sui/bin/sui-node

CMD ["/opt/sui/bin/sui-node"]

