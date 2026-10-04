#!/bin/bash

# default network is localnet
NETWORK=http://localhost:9000

# If otherwise specified chose testnet or devnet
if [ $# -ne 0 ]; then
 if [ $1 = "mainnet" ]; then
    NETWORK="https://fullnode.mainnet.sui.io:443"
  fi
  if [ $1 = "testnet" ]; then
    NETWORK="https://fullnode.testnet.sui.io:443"
  fi
  if [ $1 = "devnet" ]; then
    NETWORK="https://fullnode.devnet.sui.io:443"
  fi
fi

read -p "You are about to publish a new package at $NETWORK Continue? (y/n): " response
response=$(echo "$response" | tr '[:upper:]' '[:lower:]') # tolower
if ! [[ "$response" =~ ^(yes|y)$ ]]; then
    echo "Exiting package publishing."
    exit 1
fi

# check dependencies are available.
for i in jq sui; do
  if ! command -V ${i} 2>/dev/null; then
    echo "${i} is not installed"
    exit 1
  fi
done

publish_res=$(sui client publish --gas-budget 200000000 --json ../move_examples/nft_app --skip-dependency-verification)

echo ${publish_res} >.publish.res.json

if [[ "$publish_res" =~ "error" ]]; then
  # If yes, print the error message and exit the script
  echo "Error during move contract publishing.  Details : $publish_res"
  exit 1
fi
echo "Contract Deployment finished!"
echo "Setting up environmental variables..."

newObjs=$(echo "$publish_res" | jq -r '.objectChanges[] | select(.type == "created")')
ADMIN_CAP_ID=$(echo "$newObjs" | jq -r 'select (.objectType | contains("::genesis::AdminCap")).objectId' | head -n 1)
ADMIN_ADDRESS=$(echo "$publish_res" | jq -r '.transaction.data.sender')
PACKAGE_ID=$(echo "${publish_res}" | jq -r '.effects.created[] | select(.owner == "Immutable").reference.objectId')

cat >.test.env <<-API_ENV
NFT_APP_PACKAGE_ID=$PACKAGE_ID
NFT_APP_ADMIN_CAP=$ADMIN_CAP_ID
SUI_NODE=$NETWORK
ADMIN_ADDRESS=$ADMIN_ADDRESS
ADMIN_SECRET_KEY=
API_ENV