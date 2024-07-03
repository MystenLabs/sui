ARG SUI_TOOLS_IMAGE_TAG

FROM mysten/sui-tools:$SUI_TOOLS_IMAGE_TAG as setup

RUN apt update
RUN apt install python3 python3-pip -y

# copy configuration files to root
COPY ./new-genesis.sh /new-genesis.sh
COPY ./genesis /genesis

WORKDIR /

RUN ./new-genesis.sh

FROM scratch 

COPY ./docker-compose-antithesis.yaml /docker-compose.yaml
COPY /genesis/overlays/* /genesis/overlays/
COPY /genesis/static/* /genesis/static/
COPY --from=setup /genesis/files/* /genesis/files/
