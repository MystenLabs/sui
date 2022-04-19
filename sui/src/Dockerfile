# Get Ubuntu
FROM ubuntu:focal

RUN apt-get update
RUN DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC apt-get -y install tzdata

# Prep for Sui build
RUN apt-get install -y git curl build-essential

RUN apt-get install -y libssl-dev
RUN apt-get install -y pkg-config
RUN apt-get install -y libclang-dev

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Build Sui
WORKDIR build

RUN git clone https://github.com/MystenLabs/sui.git
WORKDIR sui

ENV PATH="/root/.cargo/bin:${PATH}"

RUN cargo build --release 

# Set the default values for args. These wont work but are needed for Dockerfile
ENV GENESIS_CFG_PATH=genesis.json
ENV ADDRESS=0

# Copy the config
COPY ${GENESIS_CFG_PATH} target/release/genesis.json
WORKDIR target/release

# Run the validator with the given address
# Eample: sudo docker run -e GENESIS_CFG_PATH=gen.json -e ADDRESS=6380E5D87A38BF2CC3945D6A3276BDD5CD91A22A -it validator

CMD ./validator --genesis-config-path genesis.json --address ${ADDRESS}