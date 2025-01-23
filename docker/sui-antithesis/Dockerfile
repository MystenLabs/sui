ARG SUI_TOOLS_IMAGE_TAG

FROM mysten/sui-tools:$SUI_TOOLS_IMAGE_TAG AS setup

RUN apt update
RUN apt install python3 python3-pip -y

# copy configuration files to root
COPY ./new-genesis.sh /new-genesis.sh
COPY ./genesis /genesis

WORKDIR /

ARG SUI_NODE_A_TAG
ARG SUI_NODE_B_TAG
ARG SUI_RUST_LOG
ENV SUI_NODE_A_TAG=$SUI_NODE_A_TAG
ENV SUI_NODE_B_TAG=$SUI_NODE_B_TAG

RUN ./new-genesis.sh
RUN echo "SUI_NODE_A_TAG=$SUI_NODE_A_TAG" >> /.env
RUN echo "SUI_NODE_B_TAG=$SUI_NODE_B_TAG" >> /.env
RUN echo "SUI_RUST_LOG=$SUI_RUST_LOG" >> /.env

COPY ./docker-compose-antithesis.yaml /docker-compose.yaml

# when running an upgrade test, we required two docker files, one for before upgrade and one for after upgrade
# One of the files runs only the primary sui-node image, and the other runs only the sui-node-alt image.
RUN <<EOF
sed 's/sui-node-alt:${SUI_NODE_B_TAG}/sui-node:${SUI_NODE_A_TAG}/' < /docker-compose.yaml > docker-compose-1.yaml
sed 's/sui-node:${SUI_NODE_A_TAG}/sui-node-alt:${SUI_NODE_B_TAG}/' < /docker-compose.yaml > docker-compose-2.yaml
EOF

FROM scratch

COPY --from=setup /docker-compose-1.yaml /docker-compose-1.yaml
COPY --from=setup /docker-compose-2.yaml /docker-compose-2.yaml
COPY --from=setup /docker-compose.yaml /docker-compose.yaml

COPY /genesis/overlays/* /genesis/overlays/
COPY /genesis/static/* /genesis/static/
COPY --from=setup /genesis/files/* /genesis/files/
COPY --from=setup /.env /.env

