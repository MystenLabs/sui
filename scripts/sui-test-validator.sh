#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

echo "sui-test-validator binary has been deprecated in favor of sui start, which is a more powerful command that allows you to start the local network with more options.
This script offers backward compatibiltiy, but ideally, you should migrate to sui start instead. Use sui start --help to see all the flags and options. 

To recreate the exact basic functionality of sui-test-validator, you must use the following options:
  * --with-faucet --> to start the faucet server on the default host and port
  * --force-regenesis --> to start the local network without persisting the state and from a new genesis

You can also use the following options to start the local network with more features:
  * --with-indexer --> to start the indexer on the default host and port. Note that this requires a Postgres database to be running locally, or you need to set the different options to connect to a remote indexer database.
  * --with-graphql --> to start the GraphQL server on the default host and port"

# holds the args names
named_args=()
config_dir=false;
indexer_port_set=false;
with_indexer=false;

# Iterate over all arguments
while [[ $# -gt 0 ]]; do
    case $1 in
       
        --with-indexer)
            with_indexer=true
            shift # Remove argument from processing
            ;;
        --config-dir=*)
            value="${1#*=}"
            named_args+=("--network.config=$value")
            config_dir=true
            shift # Remove argument from processing
            ;;
        --config-dir)
            if [[ -n $2 && $2 != --* ]]; then
                named_args+=("--network.config=$2")
                config_dir=true
                shift # Remove value from processing
            fi
            shift # Remove argument from processing
            ;;
        --faucet-port=*)
            port_value="${1#*=}"
            named_args+=("--with-faucet=$port_value")
            shift # Remove argument from processing
            ;;
        --faucet-port)
            if [[ -n $2 && $2 != --* ]]; then
                named_args+=("--with-faucet=$2")
                shift # Remove value from processing
            fi
            shift # Remove argument from processing
            ;;
        --indexer-rpc-port=*)
            port_value="${1#*=}"
            named_args+=("--with-indexer=$port_value")
            indexer_port_set=true
            shift # Remove argument from processing
            ;;
        --indexer-rpc-port)
            if [[ -n $2 && $2 != --* ]]; then
                named_args+=("--with-indexer=$2")
                indexer_port_set=true
                shift # Remove value from processing
            fi
            shift # Remove argument from processing
            ;;
        --graphql-port=*)
            port_value="${1#*=}"
            named_args+=("--with-graphql=$port_value")
            shift # Remove argument from processing
            ;;
        --graphql-port)
            if [[ -n $2 && $2 != --* ]]; then
                named_args+=("--with-graphql=$2")
                shift # Remove value from processing
            fi
            shift # Remove argument from processing
            ;;
        *)
            named_args+=("$1")
            shift # Remove unknown arguments from processing
            ;;
    esac
done

if [[ $indexer_port_set = false ]] && [[ $with_indexer = true ]]; then
  named_args+=("--with-indexer")
fi

# Basic command that replicates the command line arguments of sui-test-validator
cmd="sui start --with-faucet --force-regenesis"


# To maintain compatibility, when passing a network configuration in a directory, --force-regenesis cannot be passed.
if [ "$config_dir" = true ]; then
    echo "Starting with the provided network configuration."
    cmd="sui start --with-faucet"
fi

echo "Running command: $cmd ${named_args[@]}"
$cmd "${named_args[@]}"

