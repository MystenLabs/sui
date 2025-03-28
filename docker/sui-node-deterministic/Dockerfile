ARG PROFILE
ARG CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu

FROM stagex/pallet-rust@sha256:740b9ed5f2a897d45cafdc806976d84231aa50a64998610750b42a48f8daacab AS pallet-rust
FROM stagex/core-cross-x86_64-gnu-gcc@sha256:88a885049fddb21b48511d36b65d944322f3edfb699e95f1876b6ded8aa91da4 AS cross-x86_64-gnu-gcc
FROM stagex/core-cross-x86_64-gnu-rust@sha256:a779edf05a1de1594b83970d3e6dfa921ed04e0332e51e35850d498944b337f5 AS cross-x86_64-gnu-rust

FROM pallet-rust AS build
ARG PROFILE
ARG CARGO_BUILD_TARGET
COPY --from=cross-x86_64-gnu-gcc . /
COPY --from=cross-x86_64-gnu-rust . /
ENV RUST_BACKTRACE=1
ENV RUSTFLAGS="-C codegen-units=1"
ENV RUSTFLAGS="${RUSTFLAGS} -C target-feature=+crt-static"
ENV RUSTFLAGS="${RUSTFLAGS} -C linker=/usr/bin/x86_64-linux-gnu-gcc"

COPY . .
RUN cargo fetch
RUN --network=none cargo build --frozen --profile $PROFILE --bin sui-node

FROM scratch AS package
ARG PROFILE
ARG CARGO_BUILD_TARGET
COPY --from=build /target/${CARGO_BUILD_TARGET}/${PROFILE}/sui-node /opt/sui/bin/
