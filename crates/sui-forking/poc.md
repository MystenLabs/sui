# Forking Tool Design, Implementation, & PR execution - POC

`sui-forking` allows developers to start a local network in lock-step mode and execute transactions against some initial state derived from the live Sui network. This enables you to:

- Depend on existing on-chain packages and data
- Test contracts that interact with real deployed packages
- Develop locally while maintaining consistency with production state
- Run integration tests against forked network state and using packages deployed on the real live network

Important to note: the forking tool spins up a network that is not generating checkpoints automatically. The network requires manual intervention

# Design

### **High level diagram**

![architecture.png](architecture.png)

### gRPC Interfaces

Interacting with the forking tool is similar to interacting with a real network, and it’s made possible through gRPC. There are four main interfaces that the gRPC layer needs to implement:

**Ledger Service**
- this provides APIs for requesting & providing basic data (objects, epoch, checkpoint, transaction, etc)
- provides a service info API that is used for fetching chain id, timestamp, epoch, highest available checkpoint, version.

**State Service** 
- this provides APIs for requesting and providing data related to balances, owned objects, dynamic fields, or coin information. In the context of the forking tool, this is used as a way to access live object data through the owned objects API. 

**Transaction Execution Service**
- this provides two APIs for executing and simulation transactions

**Subscription Service**

When using the Sui CLI to interact with the forked network, the CLI requires to have a checkpoint subscription to retrieve the effects once the transaction’s effects were committed in a checkpoint. This interface has just one API, `subscribe_checkpoints`.

### Execution Engine

Under the hood, the tool uses `simulacrum` to manage the state of the network and execute transactions. In a nutshell, simulacrum has an API for creating & handling checkpoints, objects, transactions, transaction events, executing transactions, etc.

When a transaction execution request comes in from gRPC, it will be routed by the gRPC API and passed to the `simulacrum`. Before executing the transaction, there are a few more steps needed to successfully execute the transaction:
- fetch any missing input objects (this delegates fetching data to the data-layer)
- sign the transaction with a dummy private key (allows for impersonating senders)
- execute the transaction and get back the effects
- create a checkpoint
- notifies subscription service subscribers (needed for Sui CLI integration)
- return the execution results (effects and error)

### **Data Layer**

In the initial POC, the tool will have a store that persists data on disk, and resolves various queries through a GraphQL client as the backing source for historical data (checkpoints, epochs, objects, transactions).

** Checkpoints | Epochs | Transactions **

Expected read flow:
- a checkpoint|epoch|tx is identified by its sequence number|number|digest
- on read, check on disk first
- on miss, fetch from backing source
- if the data is found but is later than the fork checkpoint, return `None`
- if the data is not found, return `None`

When a checkpoint | epoch | transaction is created, it is persisted to disk. For checkpoints, a latest metadata is also updated to keep track of the latest checkpoint available on disk.
When a transaction is executed, its data and effects are persisted to disk. Each transaction also triggers a checkpoint creation, which is also persisted to disk.

** Objects **

Expected object read flow:
- an object is identified by its ID and version (or other query, e.g., latest version, version at checkpoint, etc)
- on read, check on disk first
- on miss, fetch from backing source by ID and version/query
- if the object is found but is later than the fork checkpoint, cache the latest object at the forked checkpoint and return `None`.
- if the object is not found, return `None`

When an object is updated (e.g., by executing a transaction that changes the object), the new version of the object is persisted to disk and the metadata around latest version is updated.

```rust
pub trait CheckpointReader {
    fn get(&self, sequence_number: u64) -> Result<Option<Checkpoint>, StoreError>;
    fn get_latest(&self) -> Result<Option<Checkpoint>, StoreError>;
}

pub trait CheckpointWriter {
    fn write(&self, checkpoint: Checkpoint) -> Result<(), StoreError>;
}

pub ObjectKey {
    object_id: ObjectID,
    version_query: VersionQuery
}

pub enum VersionQuery {
    Version(Option<u64>),
    RootVersion(u64),
    AtCheckpoint(u64),
}

pub trait ObjectReader {
    fn get_objects(&self, keys: [ObjectKey]) -> Result<Vec<Option<Object>>, StoreError>;
}

pub trait ObjectWriter {
    fn write(&self, object: Object) -> Result<(), StoreError>;
}

pub trait TransactionReader {
    fn get(&self, digest: &str) -> Result<Option<Transaction>, StoreError>;
}

pub trait TransactionWriter {
   fn write(&self, tx: Transaction) -> Result<(), StoreError>;
}

pub struct Epoch {
    pub epoch: u64,
    pub protocol_version: u64,
    pub reference_gas_price: u64,
    pub start_timestamp_ms: u64,
}

pub trait EpochReader {
    fn get(&self, epoch: u64) -> Result<Option<Epoch>, StoreError>;
}

pub trait EpochWriter {
    fn write(&self, epoch: Epoch) -> Result<(), StoreError>;
}
```

```rust
pub ForkStore {
    local: FileSystemStore,
    remote: GraphQLStore,
    fork_checkpoint: u64,
}

impl ForkStore {
    pub fn get_checkpoint(&self, sequence_number: u64) -> Result<Option<Checkpoint>, StoreError> {
        // check local store first, then remote store if not found
    }

    pub fn get_latest_checkpoint(&self) -> Result<Option<Checkpoint>, StoreError> {
        // check local store
    }

    pub fn write_checkpoint(&self, checkpoint: Checkpoint) -> Result<(), StoreError> {
        // persist to local store
    }

    pub fn get_objects(&self, key: [ObjectKey]) -> Result<Vec<Option<Object>>, StoreError> {
        // check local store first, then remote store if not found
    }

    pub fn write_object(&self, object: Object) -> Result<(), StoreError> {
        // persist to local store
    }

    pub fn get_transaction(&self, digest: &str) -> Result<Option<Transaction>, StoreError> {
        // check local store first, then remote store if not found
    }

    pub fn write_transaction(&self, tx: Transaction) -> Result<(), StoreError> {
        // persist to local store
    }

    pub fn get_epoch(&self, epoch: u64) -> Result<Option<Epoch>, StoreError> {
        // check local store first, then remote store if not found
    }

    pub fn get_latest_epoch(&self) -> Result<Option<Epoch>, StoreError> {
        // check local store
    }

    pub fn write_epoch(&self, epoch: Epoch) -> Result<(), StoreError> {
        // persist to local store
}
```

As data is persisted to disk, the user needs to provide a directory where the network state should be stored. Upon restarting the tool, if the directory exists and contains valid data, the tool can reuse the existing data and continue from there. This allows the user to maintain the state of the forked network across restarts.

### **Startup object seeding**

At startup, the user has the choice to seed addresses or objects, to make the forked network “aware” of them.

`--address` adds an address for seeding (works in the consistent range), loads that address's objects and adds them to the seed.
`--object` add the object by ID directly to the seed.

Note that seeding can also be done from a file:

```json
{
    "network": "testnet",
    "checkpoint": "12345678",
    "addresses": [
        "0x1234567890abcdef1234567890abcdef12345678",
        "0xabcdef1234567890abcdef1234567890abcdef12"
    ],
    "objects": [
        "0xabcdef1234567890abcdef1234567890abcdef12
    ]
}
```

At startup, the tool will also dump this information to `generated_{network}_{checkpoint}.json` for future reference. This file can be used to restart the same forked network with the same seed, which is useful for debugging or CI purposes.

### Starting a Local Forked Network

Start a local forked network at the latest checkpoint:

```bash
sui-forking start --network testnet
```

This command:

- Starts a local “*forked*” network on port 9001 (default) - this is used to interact with the forked network, e.g., advance-checkpoints, request gas, advance-clock, advance-epoch, etc. You can do so via the CLI commands with the `sui-forking` binary or via the REST API (see below).
- Starts the RPC server on port 9000 (default) - this is the gRPC endpoint you can connect the Sui client to interact with the network.
- Allows you to execute transactions against this local state and fetches objects on-demand from the real network

The command accepts a checkpoint to fork from. This must not larger than the latest known checkpoint the RPC is aware of. It will error if the user requests a checkpoint that is not available.

- `--checkpoint <number>`: The checkpoint to fork from
- `--network <network>`: Network to fork from: `mainnet (default)`, `testnet`, `devnet`, or a custom one (`-network <CUSTOM-GRAPHQL-ENDPOINT>` ). The latter is useful for “forking” from a custom local network  / private network. It requires to have a GraphQL service running and a fullnode as well.
- `--data-dir <path>`: The directory to persist the network state (checkpoints, epochs, transactions, objects, etc). This allows the user to maintain the state of the forked network across restarts.

**The startup flow**
- Initialize store layer (forking-data-store)
- Fetch the latest checkpoint (or the checkpoint specified by the user)
- Wait for commands to advance the network (checkpoint, clock) on the forking service, or gRPC requests.

### POC CLI

The forking tool provides a CLI to interact with the forking-server for various actions. In addition to the `sui-forking start` command explained previously, there are a few other commands available:

**Faucet - request SUI tokens**

```bash
sui-forking faucet --address <address> --amount <amount> # Max is 10M SUI
```

**Advance Checkpoint**

```bash
sui-forking advance-checkpoint
```

Advances the checkpoint of the local network by 1.

**Advance Clock**

```bash
sui-forking advance-clock [--milliseconds <ms>]
```

Advances the clock of the local network by 1 millisecond, or by the specified amount of milliseconds if the `--milliseconds` flag is provided.

**Check Status**

```bash
sui-forking status
```

Shows the current checkpoint, epoch, and timestamp.
