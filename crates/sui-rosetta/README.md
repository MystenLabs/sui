# Rosetta API for Sui

[Rosetta](https://www.rosetta-api.org/docs/welcome.html) is an open-source specification and set of tools for blockchain
integration. Rosetta’s goal is to make blockchain integration simpler, faster, and more reliable than using a native
integration.

## Overview

Sui-Rosetta is an implementation of the Rosetta API for the Sui network, the Sui-Rosetta server uses the Sui fullnode to
read and write transactions to the Sui network.

## Local network quick start

### Build from source
#### 0. Checkout and build Sui
Checkout the [Sui source code](https://github.com/MystenLabs/sui) and compile using `cargo build --release`, the binaries will be located in `target/release` directory.

#### 1. Genesis

`./sui genesis -f`  
The Sui genesis process will create configs and coins for testing, the config files are stored in `~/.sui/sui_config` by
default.

#### 2. Start local network

`./sui start`

#### 3. Start Rosetta Online server

`./sui-rosetta start-online-server`

#### 4. Start Rosetta Offline server

`./sui-rosetta start-offline-server`

#### 5. Generate configuration with prefunded accounts for rosetta-cli

`./sui-rosetta generate-rosetta-cli-config`
This will generate the `rosetta-cli.json` and `sui.ros` file to be used by the [Rosetta-CLI](https://github.com/coinbase/rosetta-cli)

### Build local test network using Docker Compose

#### 1. CD into the Dockerfile directory

```shell
cd <sui project directory>/crate/sui-rosetta/docker/sui-rosetta-local
```   
#### 2. Build the image
```shell
./build.sh
```
#### 3. Start the container

```shell
docker-compose up -d
```
Docker compose will start the rosetta-online and rosetta-offline containers, the ports for both rosetta server (9002, 9003 respectively) will be exposed to the host.  

#### 4. Enter the rosetta service shell

```shell
docker-compose exec rosetta-online bash
```

#### 5. use the rosetta-cli to test the api
```shell
rosetta-cli --configuration-file rosetta-cli.json check:data
rosetta-cli --configuration-file rosetta-cli.json check:construction
```

### Start Rosetta servers using docker run
you can also start the individual server using docker run.
To start the rosetta-online server, run
```shell
docker run mysten/sui-rosetta-devnet sui-rosetta start-online-server
```
Alternatively, to start the rosetta-offline server, run
```shell
docker run mysten/sui-rosetta-devnet sui-rosetta start-offline-server
```

## Supported APIs

### Account

| Method | Endpoint         | Description                    | Sui Supported? |  Server Type  |
|--------|------------------|--------------------------------|:--------------:|:-------------:|
| POST   | /account/balance | Get an Account's Balance       |      Yes       |    Online     |
| POST   | /account/coins   | Get an Account's Unspent Coins |      Yes       |    Online     |

### Block

| Method | Endpoint           | Description             |                                      Sui Supported?                                       |  Server Type  |
|--------|--------------------|-------------------------|:-----------------------------------------------------------------------------------------:|:-------------:|
| POST   | /block             | Get a Block             | Yes (One transaction per block in phase 1, will be replaced by Sui checkpoint in phase 2) |    Online     |
| POST   | /block/transaction | Get a Block Transaction |                                            Yes                                            |    Online     |

### Call

| Method | Endpoint | Description                            | Sui Supported? | Server Type |
|--------|----------|----------------------------------------|:--------------:|:-----------:|
| POST   | /call    | Make a Network-Specific Procedure Call |       No       |     --      |

### Construction

| Method | Endpoint                 | Description                                           | Sui Supported? | Server Type |
|--------|--------------------------|-------------------------------------------------------|:--------------:|:-----------:|
| POST   | /construction/combine    | Create Network Transaction from Signatures            |      Yes       |   Offline   |
| POST   | /construction/derive     | Derive an AccountIdentifier from a PublicKey          |      Yes       |   Offline   |
| POST   | /construction/hash       | Get the Hash of a Signed Transaction                  |      Yes       |   Offline   |
| POST   | /construction/metadata   | Get Metadata for Transaction Construction             |      Yes       |   Online    |
| POST   | /construction/parse      | Parse a Transaction                                   |      Yes       |   Offline   |
| POST   | /construction/payloads   | Generate an Unsigned Transaction and Signing Payloads |      Yes       |   Offline   |
| POST   | /construction/preprocess | Create a Request to Fetch Metadata                    |      Yes       |   Offline   |
| POST   | /construction/submit     | Submit a Signed Transaction                           |      Yes       |   Online    |

### Events

| Method | Endpoint       | Description                          | Sui Supported? | Server Type |
|--------|----------------|--------------------------------------|:--------------:|:-----------:|
| POST   | /events/blocks | [INDEXER] Get a range of BlockEvents |       No       |     --      |

### Mempool

| Method | Endpoint             | Description                  | Sui Supported? | Server Type |
|--------|----------------------|------------------------------|:--------------:|:-----------:|
| POST   | /mempool             | Get All Mempool Transactions |       No       |     --      |
| POST   | /mempool/transaction | Get a Mempool Transaction    |       No       |     --      |

### Network

| Method | Endpoint         | Description                    | Sui Supported? |  Server Type   |
|--------|------------------|--------------------------------|:--------------:|:--------------:|
| POST   | /network/list    | Get List of Available Networks |      Yes       | Online/Offline |
| POST   | /network/options | Get Network Options            |      Yes       | Online/Offline |
| POST   | /network/status  | Get Network Status             |      Yes       |     Online     |

### Search

| Method | Endpoint             | Description                       | Sui Supported? | Server Type |
|--------|----------------------|-----------------------------------|:--------------:|:-----------:|
| POST   | /search/transactions | [INDEXER] Search for Transactions |       No       |     --      |


## Sui transaction <> Rosetta Operation conversion explained
There are 2 places we convert Sui's transaction to Rosetta's operations, 
one is in the `/construction/parse` endpoint and another one in `/block/transaction endpoint`.
`/operation/parse` uses `Operation::from_data` to create the "intent" operations and `/block/transaction` uses `Operation::from_data_and_effect` to create the "confirmed" operations.
the `/construction/parse` endpoint is used for checking transaction correctness during transaction construction, in our case we only support `TransferSui` for now, the operations created looks like this (negative amount indicate sender):
```json
{
    "operation_identifier": {
        "index": 0
    },
    "type": "TransferSUI",
    "account": {
        "address": "0xc4173a804406a365e69dfb297d4eaaf002546ebd"
    },
    "amount": {
        "value": "-100000",
        "currency": {
            "symbol": "SUI",
            "decimals": 9
        }
    },
    "coin_change": {
        "coin_identifier": {
            "identifier": "0x0dce9190d54cde842d39537bf94efe128181b8a6:5549"
        },
        "coin_action": "coin_spent"
    }
},
{
    "operation_identifier": {
        "index": 1
    },
    "type": "TransferSUI",
    "account": {
        "address": "0x96bc0b37b67103651d1f98c67b34df9558ea527a"
    },
    "amount": {
        "value": "100000",
        "currency": {
            "symbol": "SUI",
            "decimals": 9
        }
    }
},
{
    "operation_identifier": {
        "index": 2
    },
    "type": "GasBudget",
    "account": {
        "address": "0xc4173a804406a365e69dfb297d4eaaf002546ebd"
    },
    "coin_change": {
        "coin_identifier": {
            "identifier": "0x0dce9190d54cde842d39537bf94efe128181b8a6:5549"
        },
        "coin_action": "coin_spent"
    },
    "metadata": {
        "budget": 1000
    }
}
```
The sender, recipients and transfer amounts are specified in the intent operations, 
these data are being used to create TransactionData for signature.
After the tx is executed, the rosetta-cli compare the intent operations with the confirmed operations , 
the confirmed operations must contain the intent operations (the confirmed operations can have more operations than the intent).
Since the intent operations of TransferSui contains all the balance change information(amount field) already, 
we don't need to use the event to create the operations, also operation created by `get_coin_operation_from_event` will contain recipient's coin id, which will cause a mismatch.