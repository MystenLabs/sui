# Sui (pre-alpha)

[![Build Status](https://github.com/mystenlabs/fastnft/actions/workflows/rust.yml/badge.svg)](https://github.com/mystenlabs/fastnft/actions/workflows/rust.yml)
[![License](https://img.shields.io/badge/license-Apache-green.svg)](LICENSE.md)

This repository is dedicated to sharing material related to the Sui protocol, developed at Novi Financial (formerly Calibra). Software is provided for research-purpose only and is not meant to be used in production.

## Summary

Sui extends Sui by allowing objects to be transacted.

Sui allows a set of distributed authorities, some of which are Byzantine, to maintain a high-integrity and availability settlement system for pre-funded payments. It can be used to settle payments in a native unit of value (crypto-currency), or as a financial side-infrastructure to support retail payments in fiat currencies. Sui is based on Byzantine Consistent Broadcast as its core primitive, foregoing the expenses of full atomic commit channels (consensus). The resulting system has low-latency for both confirmation and payment finality. Remarkably, each authority can be sharded across many machines to allow unbounded horizontal scalability. Our experiments demonstrate intra-continental confirmation latency of less than 100ms, making Sui applicable to point of sale payments. In laboratory environments, we achieve over 80,000 transactions per second with 20 authorities---surpassing the requirements of current retail card payment networks, while significantly increasing their robustness.

## Quickstart with local Sui network and interactive wallet

### 1. Build the binaries

```shell
cargo build --release
cd target/release
```

This will create `sui` and `wallet` binaries in `target/release` directory.

### 2. Genesis

```shell
./sui genesis
```

The genesis command creates 4 authorities, 5 user accounts each with 5 gas objects.  
The network configuration are stored in `network.conf` and can be used subsequently to start the network.  
A `wallet.conf` will also be generated to be used by the `wallet` binary to manage the newly created accounts.  

### 2.1 Genesis customization

The genesis process can be customised by providing a genesis config file.

```shell
./sui genesis --config genesis.conf
```
Example `genesis.conf`
```json
{
  "authorities": [
    {
      "key_pair": "xWhgxF5fagohi2V9jzUToxnhJbTwbtV2qX4dbMGXR7lORTBuDBe+ppFDnnHz8L/BcYHWO76EuQzUYe5pnpLsFQ==",
      "host": "127.0.0.1",
      "port": 10000,
      "db_path": "./authorities_db/4e45306e0c17bea691439e71f3f0bfc17181d63bbe84b90cd461ee699e92ec15",
      "stake": 1
    }
  ],
  "accounts": [
    {
      "address": "bd654f352c895d9ec14c491d3f2b4e1f98fb07323383bebe9f95ab625bff2fa0",
      "gas_objects": [
        {
          "object_id": "5c68ac7ba66ef69fdea0651a21b531a37bf342b7",
          "gas_value": 1000
        }
      ]
    }
  ],
  "move_packages": ["<Paths to custom move packages>"],
  "sui_framework_lib_path": "<Paths to sui framework lib>",
  "move_framework_lib_path": "<Paths to move framework lib>"
}
```
All attributes in genesis.conf are optional, default value will be use if the attributes are not provided.  
For example, the config shown below will create a network of 4 authorities, and pre-populate 2 gas objects for 4 accounts.
```json
{
  "authorities": [
    {},{},{},{}
  ],
  "accounts": [
    { "gas_objects":[{},{}] },
    { "gas_objects":[{},{}] },
    { "gas_objects":[{},{}] },
    { "gas_objects":[{},{}] }
  ]
}
```

### 3. Starting the network

Run the following command to start the local Sui network:

```shell
./sui start 
```

or

```shell
./sui start --config [config file path]
```

The network config file path is defaulted to `./network.conf` if not specified.  

### 4. Running interactive wallet

To start the interactive wallet:

```shell
./wallet
```

or

```shell
./wallet --config [config file path]
```

The wallet config file path is defaulted to `./wallet.conf` if not specified.  

The following commands are supported by the interactive wallet:

    addresses      Obtain the Account Addresses managed by the wallet
    call           Call Move
    help           Prints this message or the help of the given subcommand(s)
    new-address    Generate new address and keypair
    object         Get obj info
    objects        Obtain all objects owned by the account address
    publish        Publish Move modules
    sync           Synchronize client state with authorities
    transfer       Transfer funds

Use `help <command>` to see more information on each command.

### 5. Using the wallet without interactive shell

The wallet can also be use without the interactive shell

```shell
USAGE:
    wallet --no-shell [SUBCOMMAND]
```

#### Example

```shell
sui@MystenLab release % ./wallet --no-shell addresses                                                                         
Showing 5 results.
4f145f9a706ae4932452c90ce006fbddc8ab2ced34584f26d8953df14a76463e
47ea7f45ca66fc295cd10fbdf8a41828db2ed71c145476c710e04871758f48e9
e91c22628771f1465947fe328ed47983b1a1013afbdd1c8ded2009ec4812054d
9420c11579a0e4a75a48034d9617fd68406de4d59912e9d08f5aaf5808b7013c
1be661a8d7157bffbb2cf7f652d270bbefb07b0b436aa10f2c8bdedcadcc22cb
```

## References

* Sui is based on FastPay: [FastPay: High-Performance Byzantine Fault Tolerant Settlement](https://arxiv.org/pdf/2003.11506.pdf)

## Contributing

Read [Eng Plan](https://docs.google.com/document/d/1Cqxaw23PR2hc5bkbhXIDCnWjxA3AbfjsuB45ltWns4U/edit#).

## License

The content of this repository is licensed as [Apache 2.0](https://github.com/MystenLabs/fastnft/blob/update-readme/LICENSE)
