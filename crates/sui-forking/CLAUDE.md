# Description

Sui Forking is a tool that allows developers to start a local network in lock-step mode and execute transactions against some initial state derived from the actual Sui network (mainnet, testnet, or devnet). The purpose of this tool
is to enable users to depend on on-chain packages and data, while being able to test and develop in a local environment. 

Compared to a normal local Sui network, in this tool there are no validators because we do not have access to the keys of the validators on the actual network. Instead, the local network runs in lock-step mode, where each transaction is executed one at a time. When a transaction is executed, a checkpoint is created. Checkpoints can be advanced also manually through a command.

# Usage

```bash
sui fork --checkpoint 100 --network testnet # this will start a local network on port 8123 and allow to download the objects needed at checkpoint 100 on testnet. The developer can then initiate transactions against this local network.

sui move test --fork --checkpoint 100 --network testnet # this will run the tests against a local forking network at checkpoint 100 on testnet by downloading the required objects for the transaction, and then executing the transaction locally.
```

# Architecture
The Sui Forking tool consists of the following main components:
- a data store similar to the one in `../sui-replay-2` that is responsible for downloading and caching objects from the actual network via GraphQL RPC calls. Note, that we cannot download all objects at that checkpoint, but instead, we will need
to determine which objects we need to fetch either from cache (if exists), or from the network in order to execute the transaction. To understand more about the transaction itself, see `../sui-types` and `../sui-sdk` crates.
- a simulacrum that is able to start a local Sui network in lock-step mode, and execute transactions against it. A form of this already exists in `../simulacrum/`, and probably we should extend that with whatever APIs we need and use it here.
- a coordinator that is responsible for coordinating between the data store and the simulacrum. The coordinator is responsible for:
  - initializing the local network with the required packages and objects at a given checkpoint.
  - intercepting transaction execution requests, and ensuring that all required objects are available in the local data store before executing the transaction in the local network.
  - advancing checkpoints in the local network when requested.


# Commands
The Sui Forking tool provides the following commands:
```bash
sui fork --checkpoint <checkpoint> --network <network> [--port <port>] [--data-dir <data-dir>]
sui fork advance-checkpoint # advances the checkpoint of the local network by 1
sui fork advance-clock # advances the clock of the local network by 1 second
sui fork advance-epoch # advances the epoch of the local network by 1
sui fork status # shows the current checkpoint and status of the local network
sui move test --fork --checkpoint <checkpoint> --network <network> [other sui move test args] # runs the tests against a local forking network at the given checkpoint and network, and exists once the tests are done.
```

# Initial state
- ownership information that you care about - accounts that the user requires


# Data Fetching
- check local cache by object ID and version (that is one kind of query)
    - it can also be the latest object at this checkpoint. This is required for shared objects (in general, shared objects can have multiple versions because of consensus scheduling - during consensus, after ordering,
    each transaction will be assigned with a version of the shared object that will be used for execution).
- get latest at checkpoint - this is needed for packages, so we need the latest at the checkpoint
- for dynamic fields you need to get find the object at the parent version


# Caching Strategy
- local cache is at the latest forked checkpoint
