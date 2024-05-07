# We generate the genesis blob and validator configurations
from docker.io/mysten/sui-tools:mainnet-v1.19.1 as setup

RUN apt update
RUN apt install python3 python3-pip -y

# copy configuration files to root
COPY ./new-genesis.sh /new-genesis.sh
COPY ./genesis /genesis

WORKDIR /

RUN ./new-genesis.sh

FROM scratch 

COPY ./docker-compose.yaml /
COPY /genesis/overlays/* /genesis/overlays/
COPY /genesis/static/* /genesis/static/
COPY --from=setup /genesis/files/* /genesis/files/
