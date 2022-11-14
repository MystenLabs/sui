# Run a Sui DevNet Fullnode Locally with Docker

Run a Sui DevNet [fullnode](../../doc/src/build/fullnode.md) locally for testing/experimenting by following the instructions below. This has been tested and should work for:

- linux/amd64
- darwin/amd64
- darwin/arm64

## Prerequisites

Install Docker / Docker Compose:
- https://docs.docker.com/get-docker/
- https://docs.docker.com/compose/install/
- https://github.com/MystenLabs/sui/blob/main/docker/fullnode/docker-compose.yaml

## Running

### Fullnode config

Download the latest version of the fullnode config [fullnode-template.yaml](https://github.com/MystenLabs/sui/raw/main/crates/sui-config/data/fullnode-template.yaml) over the web or by using `curl` or `wget`, for example:

```shell
wget https://github.com/MystenLabs/sui/raw/main/crates/sui-config/data/fullnode-template.yaml
```

### sui devnet genesis

Get the latest version of the Sui DevNet genesis [genesis.blob](https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob) file over the web or:

```wget https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob```


## Start the fullnode

> **Important:** This document reflects Docker Compose V1. If you are using [Docker Compose V2](https://docs.docker.com/compose/#compose-v2-and-the-new-docker-compose-command), replace the hyphen (-) in the `docker-compose` commands below with a space, like so: `docker compose`

To start the fullnode using Docker, run:

```shell
docker-compose up
```

## Test

Once the fullnode is up and running, test some of the JSON-RPC interfaces.

## Use your fullnode with Explorer

To use the Sui Explorer with your fullnode, follow these steps:
1. Open a browser and go to: https://explorer.sui.io/
1. Click the **Devnet** button in the top right-hand corner of the Explorer and select
   the *Local* network from the drop-down menu.
1. Close the *Choose a Network* menu to see the latest transactions.

## Troubleshoot / tips / documentation

### Start the fullnode in detached mode

```docker-compose up -d```

### Stop the fullnode

```docker-compose stop```

### Reset the environment

Take everything down, removing the container and volume. Use this to start completely fresh (image, config, or genesis updates):

```docker-compose down --volumes```

### Inspect the state of a running fullnode

Get the running container ID:

```docker ps```

Connect to a bash shell inside the container:

```docker exec -it $CONTAINER_ID /bin/bash```

Inspect the database:

```ls -la suidb/```

### Investigate local RPC connectivity issues

Update the `json-rpc-address` in the fullnode config to listen on all addresses:

```sed -i 's/127.0.0.1/0.0.0.0/' fullnode-template.yaml```

```
-json-rpc-address: "127.0.0.1:9000"
+json-rpc-address: "0.0.0.0:9000"
```

### Install wget and curl

Download each package. For example, on macOS use [homebrew](https://brew.sh/):

```brew install wget curl```

### Learn more about Sui
- https://docs.sui.io/learn

### Learn more about building and running a fullnode natively
- https://docs.sui.io/build/fullnode

### Learn more about docker-compose
- https://docs.docker.com/compose/gettingstarted/
