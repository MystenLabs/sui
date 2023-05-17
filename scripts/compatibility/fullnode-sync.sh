#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -e

DEFAULT_NETWORK="testnet"
CLEAN=0
LOG_LEVEL="info"
SUI_RUN_PATH="/opt/sui"
VERBOSE=""

function cleanup {
    echo "Performing exit cleanup..."
    [ ! -z $SUI_NODE_PID ] && kill $SUI_NODE_PID && echo "Shutdown suinode process running on pid $SUI_NODE_PID"
}

trap cleanup EXIT

while getopts "hvn:e:p:t:" OPT; do
    case $OPT in
        p) 
            SUI_BIN_PATH=$OPTARG ;;
        v)
            LOG_LEVEL="sui=debug,error"
            VERBOSE="--verbose" 
            ;;
        n)
            NETWORK=$OPTARG ;;
        e)
            END_EPOCH=$OPTARG ;;
        t)
            EPOCH_TIMEOUT=$OPTARG ;;
        h)
            >&2 echo "Usage: $0 [-h] [-p] [-v] [-n NETWORK] [-e END_EPOCH] [-t EPOCH_TIMEOUT]"
            >&2 echo ""
            >&2 echo "Options:"
            >&2 echo " -p                 Path to sui binary to run. If unspecified, will build from source."
            >&2 echo " -v                 Run with verbose logging."
            >&2 echo " -n NETWORK         The network to run the fullnode on."
            >&2 echo "                    (Default: ${DEFAULT_NETWORK})"
            >&2 echo " -e END_EPOCH       EpochID at which to stop syncing and declare success."
            >&2 echo "                    If unspecified or -1, will use current epoch of NETWORK."
            >&2 echo " -t EPOCH_TIMEOUT   Number of minutes to wait until epoch advancement before timing out."
            >&2 exit 0
            ;;
        \?)
            >&2 echo "Unrecognized option '$OPTARG'"
            exit 1
            ;;
    esac
done

[ ! -d "${SUI_RUN_PATH}/suidb" ] && mkdir -p ${SUI_RUN_PATH}/suidb

if [[ -z "$NETWORK" ]]; then
    NETWORK=$DEFAULT_NETWORK
elif [[ "$NETWORK" != "testnet" && "$NETWORK" != "devnet" ]]; then
    >&2 echo "Invalid network ${NETWORK}"
    exit 1
fi

if [[ ! -f "${SUI_RUN_PATH}/genesis.blob" ]]; then
    echo "Copying genesis.blob for ${NETWORK}"
    curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/${NETWORK}/genesis.blob
    mv ./genesis.blob ${SUI_RUN_PATH}/genesis.blob
    echo "Done"
fi

if [[ ! -f "${SUI_RUN_PATH}/fullnode.yaml" ]]; then
    echo "Generating fullnode.yaml at ${SUI_RUN_PATH}/fullnode.yaml"
    cp crates/sui-config/data/fullnode-template.yaml ${SUI_RUN_PATH}/fullnode.yaml
    sed -i "s|genesis.blob|${SUI_RUN_PATH}/genesis.blob|g" ${SUI_RUN_PATH}/fullnode.yaml
    sed -i "s|suidb|${SUI_RUN_PATH}/suidb|g" ${SUI_RUN_PATH}/fullnode.yaml

    if [[ $NETWORK != "devnet" ]]; then
        cat >> "${SUI_RUN_PATH}/fullnode.yaml" <<- EOM

p2p-config:
  seed-peers:
    - address: /dns/ewr-tnt-ssfn-00.testnet.sui.io/udp/8084
      peer-id: df8a8d128051c249e224f95fcc463f518a0ebed8986bbdcc11ed751181fecd38
    - address: /dns/lax-tnt-ssfn-00.testnet.sui.io/udp/8084
      peer-id: f9a72a0a6c17eed09c27898eab389add704777c03e135846da2428f516a0c11d
    - address: /dns/lhr-tnt-ssfn-00.testnet.sui.io/udp/8084
      peer-id: 9393d6056bb9c9d8475a3cf3525c747257f17c6a698a7062cbbd1875bc6ef71e
    - address: /dns/mel-tnt-ssfn-00.testnet.sui.io/udp/8084
      peer-id: c88742f46e66a11cb8c84aca488065661401ef66f726cb9afeb8a5786d83456e
EOM
    fi
    
    echo "Done"
fi

if [[ -z $SUI_BIN_PATH ]]; then
    echo "Building sui..."
    cargo build --release --bin sui-node
    SUI_BIN_PATH="target/release/sui-node"
    echo "Done"
fi


echo "Starting suinode..."
RUST_LOG=$LOG_LEVEL $SUI_BIN_PATH --config-path ${SUI_RUN_PATH}/fullnode.yaml &
SUI_NODE_PID=$!

# start monitoring script
END_EPOCH_ARG=""
if [[ ! -z $END_EPOCH && $END_EPOCH != -1 ]]; then
    END_EPOCH_ARG="--end-epoch $END_EPOCH"
fi

EPOCH_TIMEOUT_ARG=""
if [[ ! -z $EPOCH_TIMEOUT ]]; then
    EPOCH_TIMEOUT_ARG="--epoch-timeout $EPOCH_TIMEOUT"
fi

./scripts/compatibility/monitor_synced.py $END_EPOCH_ARG $EPOCH_TIMEOUT_ARG --env $NETWORK $VERBOSE

kill $SUI_NODE_PID
exit 0

