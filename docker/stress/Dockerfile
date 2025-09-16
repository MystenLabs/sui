ARG PROFILE=release
ARG GIT_REVISION

FROM rust:1.85-bullseye AS builder
ENV GIT_REVISION=$GIT_REVISION
WORKDIR /sui
RUN apt-get update && apt-get install -y cmake clang
COPY Cargo.toml Cargo.lock ./
COPY consensus consensus
COPY crates crates
COPY sui-execution sui-execution
COPY external-crates external-crates
COPY examples examples
RUN cargo build --profile ${PROFILE} --bin stress

FROM rust:1.85-bullseye

COPY --from=builder /sui/target/${PROFILE}/stress /usr/local/bin
COPY --from=builder /sui/examples/move /examples/move

RUN apt-get update && apt-get -y --no-install-recommends install wget=1.21-1+deb11u1 \
        iputils-ping netcat procps bind9-host bind9-dnsutils curl iproute2 git ca-certificates awscli

ARG GIT_REVISION
RUN echo ${GIT_REVISION:-$SUI_TOOLS_IMAGE_TAG} > /var/run/sui_commit

COPY ./entrypoint.sh .

CMD ["./entrypoint.sh"]
