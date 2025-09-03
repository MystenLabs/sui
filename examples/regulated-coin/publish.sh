#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0


# check dependencies are available.
for i in jq curl sui; do
  if ! command -V ${i} 2>/dev/null; then
    echo "${i} is not installed"
    exit 1
  fi
done

# Put the dependant package, as the depending will be published too via --with-unpublished-dependencies
MOVE_PACKAGE_PATH=./move

sui client switch --env testnet

NETWORK="https://rpc.testnet.sui.io:443"
FAUCET="https://faucet.testnet.sui.io/gas"
    
sui client switch --env testnet

ADMIN_ADDRESS=$(sui client active-address)

echo "- Publisher Address is: ${ADMIN_ADDRESS}"

publish_res=$(sui client publish --json ${MOVE_PACKAGE_PATH})

echo ${publish_res} >.publish.res.json

# Check if the command succeeded (exit status 0)
if [[ "$publish_res" =~ "error" ]]; then
  # If yes, print the error message and exit the script
  echo "Error during move contract publishing.  Details : $publish_res"
  exit 1
fi

publishedObjs=$(echo "$publish_res" | jq -r '.objectChanges[] | select(.type == "published")')
PACKAGE_ID=$(echo "$publishedObjs" | jq -r '.packageId')

newObjs=$(echo "$publish_res" | jq -r '.objectChanges[] | select(.type == "created")')
DENY_CAP_ID=$(echo "$newObjs" | jq -r 'select(.objectType | contains("::coin::DenyCapV2<")).objectId')
TREASURY_CAP_ID=$(echo "$newObjs" | jq -r 'select(.objectType | contains("::coin::TreasuryCap<")).objectId')

suffix=""
if [ $# -eq 0 ]; then
  suffix=".localnet"
fi

cat >ts-client/.env<<-API_ENV
SUI_FULLNODE_URL=$NETWORK
PACKAGE_ID=$PACKAGE_ID
ADMIN_ADDRESS=$ADMIN_ADDRESS
DENY_CAP_ID=$DENY_CAP_ID
TREASURY_CAP_ID=$TREASURY_CAP_ID
MODULE_NAME=regulated_coin
COIN_NAME=REGULATED_COIN
API_ENV

cat >rust-client/.env<<-API_ENV
SUI_FULLNODE_URL=$NETWORK
PACKAGE_ID=$PACKAGE_ID
MODULE_NAME=regulated_coin
RUST_LOG=rust_client=DEBUG
API_ENV

echo "Contract deployment finished!"
