FROM rust:1.92-bullseye AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake clang libpq5 libpq-dev \
 && rm -rf /var/lib/apt/lists/*
ARG PROFILE=release
WORKDIR /sui

# COPYs ordered least- to most-frequently-changing for deeper layer cache hits.
COPY rust-toolchain.toml ./
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY external-crates external-crates
COPY sui-execution sui-execution
COPY consensus consensus
COPY crates crates

# stress binary doesn't use GIT_REVISION at compile time, so we deliberately
# don't declare ARG/ENV GIT_REVISION in the builder — keeps the cargo-build
# layer cache key stable across commits when source content is unchanged.
RUN cargo build --locked --profile ${PROFILE} --bin stress

FROM rust:1.92-bullseye
ARG PROFILE=release
ARG GIT_REVISION
COPY --from=builder /sui/target/${PROFILE}/stress /usr/local/bin
WORKDIR "$WORKDIR/sui"

RUN apt-get update && apt-get -y --no-install-recommends install wget \
        iputils-ping netcat procps bind9-host bind9-dnsutils curl iproute2 git ca-certificates awscli

RUN git clone https://github.com/MystenLabs/sui.git ; \
        cd sui ; \
        git checkout $GIT_REVISION ; \
        cd .. ; \
        mv sui/* .

RUN mkdir -p /docker/stress
RUN cp docker/stress/entrypoint.sh /docker/stress/entrypoint.sh
RUN chmod +x /docker/stress/entrypoint.sh
RUN echo ${GIT_REVISION:-$GIT_REVISION} > /var/run/sui_commit

CMD ["/docker/stress/entrypoint.sh"]
