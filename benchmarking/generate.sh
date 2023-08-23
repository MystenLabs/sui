#!/bin/bash

# Parameters check
if [[ $1 == "" ]]
then
  echo "Please provide a name for the benchmark."
  echo "Usage: $0 [benchmark_name]"
  exit
fi

# Warn user about possible data loss.
goon=""
echo -e "\033[33mContinuing will delete the directory ~/.sui and everything in it!\033[0m"
echo -n "Do you wish to continue? [y/N] "
read -r goon
if [[ $goon != 'y' ]]
then
  exit
fi

# Clean up old data.
rm -r ~/.sui

# Run local testnet and pass transactions to full node from TypeScript SDK client
# in order to generate a workload for later benchmark executions.
./sui genesis
./sui start &> ./data/logs/sui.out &

sleep 5

./sui-faucet --write-ahead-log ./data/faucet.wal --port 9123 --amount 1000000000000 --max-request-per-second 10000 --request-buffer-size 10000 &> ./data/logs/faucet.out &

sleep 5

bun workload_generation.ts

# Kill sui & sui-faucet after workload generation script finished.
kill %1
kill %2

# Copy results to data directory.
cp ~/.sui/sui_config/genesis.blob "./data/genesis/$1.blob"
mkdir -p "data/txs/$1"
cp -r ~/.sui/sui_config/authorities_db/8dcff6d15504/live "./data/txs/$1/"

echo -n "db-path: \"./data/txs/$1\"

network-address: \"/dns/localhost/tcp/8085/http\"
metrics-address: \"0.0.0.0:9185\"
json-rpc-address: \"0.0.0.0:9001\"
enable-event-processing: true

genesis:
  genesis-file-location: \"./data/genesis/$1.blob\"" > "./data/config/$1.yaml"
