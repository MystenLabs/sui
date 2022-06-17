# run a sui devnet fullnode locally

Run a Sui DevNet fullnode locally for testing/experimenting by following the instructions below. This has been tested and should work for:

- linux/amd64
- darwin/amd64 
- darwin/arm64

# prerequisites

Install docker / docker compose:
- https://docs.docker.com/get-docker/
- https://docs.docker.com/compose/install/

# run

## fullnode config

Get the latest version of the fullnode config [here](https://github.com/MystenLabs/sui/raw/main/crates/sui-config/data/fullnode-template.yaml), or:

```wget https://github.com/MystenLabs/sui/raw/main/crates/sui-config/data/fullnode-template.yaml```

## sui devnet genesis

Get the latest version of the Sui DevNet genesis blob [here](https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob), or:

```wget https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob```


## start the fullnode

```docker-compose up```

# test

Once the fullnode is up and running, test some of the jsonrpc interfaces.

- get the five most recent transactions:

```
curl --location --request POST 'http://127.0.0.1:9000/' \
    --header 'Content-Type: application/json' \
    --data-raw '{ "jsonrpc":"2.0", "id":1, "method":"sui_getRecentTransactions", "params":[5] }'
```

- get details about a specific transaction:

```
curl --location --request POST 'http://127.0.0.1:9000/' \
    --header 'Content-Type: application/json' \
    --data-raw '{ "jsonrpc":"2.0", "id":1, "method":"sui_getTransaction", "params":["$RECENT_TXN_FROM_ABOVE"] }'
```

# use your fullnode with explorer 

To use the DevNet explorer with your fullnode follow these steps:
- Open https://explorer.devnet.sui.io
- Click the green Devnet button in the top right
- Select 'Custom RPC URL'
- Set it to http://127.0.0.1:9000

# troubleshoot / tips / documentation

## start the fullnode in detached mode

```docker-compose up -d```

## stop the fullnode:

```docker-compose stop```

## reset the environment

Take everything down, removing the container and volume. Use this to start completely fresh (image, config, or genesis updates):

```docker-compose down --volumes```

## inspect the state of a running fullnode

Get the running container id:

```docker ps```

Connect to a bash shell inside the container:

```docker exec -it $CONTAINER_ID /bin/bash```

Inspect the database:

```ls -la suidb/```

## local rpc connectivity issues

Update the json-rpc-address in the fullnode config to listen on all addresses:

```sed -i 's/127.0.0.1/0.0.0.0/' fullnode-template.yaml```

```
-json-rpc-address: "127.0.0.1:9000"
+json-rpc-address: "0.0.0.0:9000"
```

## install wget and curl

On MacOS using [homebrew](https://brew.sh/):

```brew install wget curl```

## learn more about sui
- https://docs.sui.io/learn

## learn more about building and running a fullnode natively
- https://docs.sui.io/build/fullnode

## learn more about docker-compose
- https://docs.docker.com/compose/gettingstarted/