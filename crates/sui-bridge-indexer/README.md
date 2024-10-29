## Overview

Sui Bridge Indexer is a binary that scans Sui Bridge transactions on Sui and Ethereum networks, and indexes the processed data for further use.

## Get Binary

```bash
cargo build --bin bridge-indexer --release
```

The pre-built Docker image for Bridge Indexer can be found in `mysten/sui-tools:{SHA}`

## Run Binary

```
bridge-indexer --config-path config.yaml
```


## Config

```yaml
---
remote_store_url: https://checkpoints.mainnet.sui.io
eth_rpc_url: {eth rpc url}
sui_rpc_url: {sui rpc url}

concurrency: 500
checkpoints_path: {path-for-checkpoints}

eth_sui_bridge_contract_address: 0xda3bD1fE1973470312db04551B65f401Bc8a92fD # <-- mainnet, 0xAE68F87938439afEEDd6552B0E83D2CbC2473623 for testnet
metric_port: {port to export metrics}

sui_bridge_genesis_checkpoint: 55455583 # <-- mainnet, 43917829 for testnet
# genesis block number for eth
eth_bridge_genesis_block: 20811249 # <-- mainnet, 5997013 for testnet

eth_ws_url: {eth websocket url}

```

