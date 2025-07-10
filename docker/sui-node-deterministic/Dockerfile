ARG PROFILE
ARG CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu

FROM stagex/pallet-rust@sha256:740b9ed5f2a897d45cafdc806976d84231aa50a64998610750b42a48f8daacab AS pallet-rust
FROM stagex/core-cross-x86_64-gnu-gcc@sha256:88a885049fddb21b48511d36b65d944322f3edfb699e95f1876b6ded8aa91da4 AS cross-x86_64-gnu-gcc
FROM stagex/core-cross-x86_64-gnu-rust@sha256:12d60727f597a67a5ca8ec95c3ec7a6df13c516df30d1d98b4296308f430b1e1 AS cross-x86_64-gnu-rust
FROM stagex/core-cross-x86_64-gnu-libunwind@sha256:3a5a3e2049d56830164b2677df8d96a1081209a6ff59ccecd5c743f69ae5008e AS cross-x86_64-gnu-libunwind
FROM stagex/core-clang@sha256:abf6d2868bc441b5910ef28f38123c6053391521948b33eaf68980fb8be7d105 AS clang
FROM stagex/core-llvm@sha256:bc1c6d67aa73a96dd92f5def7e2701de78b0639d0c815d69110fbb9b3b3e85fe AS llvm
FROM stagex/core-lld@sha256:a5cb61edc071d404cd33cb0b5c7113a334cb90ca203cb40fe6cafd3559b6daa5 AS lld
FROM stagex/core-libffi@sha256:9acd18e59ca11fa727670725e69a976d96f85a00704dea6ad07870bff2bd4e8b AS libffi

FROM pallet-rust AS build
ARG PROFILE
ARG CARGO_BUILD_TARGET
COPY --from=cross-x86_64-gnu-gcc . /
COPY --from=cross-x86_64-gnu-rust . /
COPY --from=cross-x86_64-gnu-libunwind . /
COPY --from=clang . /
COPY --from=llvm . /
COPY --from=lld . /
COPY --from=libffi . /

ENV RUST_BACKTRACE=1
ENV RUSTFLAGS="-C codegen-units=1"
ENV RUSTFLAGS="${RUSTFLAGS} -C target-feature=+crt-static"
ENV RUSTFLAGS="${RUSTFLAGS} -C linker=/usr/bin/clang"
ENV RUSTFLAGS="${RUSTFLAGS} -C link-arg=-fuse-ld=lld -C link-arg=-Wl,--threads=1 -C link-arg=-L/opt/cross/x86_64-linux-gnu/lib"
ENV RUSTFLAGS="${RUSTFLAGS} -C link-arg=-Wl,-z,nostart-stop-gc"

COPY . .
RUN cargo fetch
RUN --network=none cargo build --frozen --profile $PROFILE --bin sui-node

FROM scratch AS package
ARG PROFILE
ARG CARGO_BUILD_TARGET
COPY --from=build /target/${CARGO_BUILD_TARGET}/${PROFILE}/sui-node /opt/sui/bin/
