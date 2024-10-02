ARG SUI_TOOLS_IMAGE_TAG

FROM mysten/sui-tools:$SUI_TOOLS_IMAGE_TAG

ARG SUI_TOOLS_IMAGE_TAG

RUN apt-get update && apt-get -y --no-install-recommends install wget=1.21-1+deb11u1 \
        iputils-ping netcat procps bind9-host bind9-dnsutils curl iproute2 git ca-certificates awscli

# stress needs access to examples/move/basics
RUN git clone https://github.com/MystenLabs/sui.git ; \
        cd sui ; \
        git checkout $SUI_TOOLS_IMAGE_TAG ; \
        cd .. ; \
        mv sui/* .

ARG SUI_TOOLS_IMAGE_TAG
RUN echo $SUI_TOOLS_IMAGE_TAG > /var/run/sui_commit

COPY ./entrypoint.sh .

CMD ["./entrypoint.sh"]
