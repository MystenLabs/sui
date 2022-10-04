FROM ubuntu:latest AS chef
WORKDIR sui
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
RUN apt-get update && apt-get install -y build-essential libssl-dev pkg-config curl cmake clang git
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

FROM chef AS builder
RUN git clone https://github.com/MystenLabs/sui .
# We can change this to `git checkout -b devnet` after next devnet update
RUN git checkout d55cce4b56e8a6c329f56c09adf87ed26ef9c126

RUN curl -sSfL https://raw.githubusercontent.com/coinbase/rosetta-cli/master/scripts/install.sh | sh -s
RUN curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob
RUN cargo build --release --bin sui --bin sui-rosetta

# Production Image
FROM ubuntu:latest AS runtime
WORKDIR sui
COPY --from=builder /sui/target/release/sui /usr/local/bin
COPY --from=builder /sui/target/release/sui-rosetta /usr/local/bin
COPY --from=builder /sui/bin/rosetta-cli /usr/local/bin
COPY --from=builder /sui/crates/sui-config/data/fullnode-template.yaml /sui/devnet/fullnode.yaml
COPY --from=builder /sui/genesis.blob /sui/devnet/genesis.blob
RUN /usr/local/bin/sui genesis

ARG BUILD_DATE
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION
