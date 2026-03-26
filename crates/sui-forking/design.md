# Forking Tool Design, Implementation, & PR execution

`sui-forking` allows developers to start a local network in lock-step mode and execute transactions against some initial state derived from the live Sui network. This enables you to:

- Depend on existing on-chain packages and data
- Test contracts that interact with real deployed packages
- Develop locally while maintaining consistency with production state
- Run integration tests against forked network state and using packages deployed on the real live network

Important to note: the forking tool spins up a network that is not generating checkpoints automatically. The network requires manual intervention

# Design

### **High level diagram**

![image.png](image.png)

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
- fetch any missing input objects (this delegates fetching to the data-layer)
- sign the transaction with a dummy private key (allows for impersonating senders)
- execute the transaction and get back the effects
- create a checkpoint
- notifies subscription service subscribers (needed for Sui CLI integration)
- return the execution results (effects and error)

### **Data Layer**

`simulacrum` requires to use a `store` where all the required data (checkpoints, transactions, objects, etc) lives. To this end, a `ServiceStore` type that implements the required traits from `simulacrum` (e.g., `SimulatorStore`, `BackingStore`, `ObjectStore`, etc.), is implemented.

Ultimately, the goal is to get to something like this for this store:

```jsx
pub struct ServiceStore {
    /// Capability-routed composite store:
    /// - transactions/epochs/checkpoints: memory -> filesystem (note in POC there is no historical data provided)
    /// - objects: memory -> filesystem -> GraphQL
    store: ForkDataStore,

    // The checkpoint at which this forked network was forked
    forked_at_checkpoint: u64,

    /// Optional validator-set override used when building epoch state for checkpoint production.
    /// This keeps the committee aligned with locally available validator keys in forking mode.
    validator_set_override: Option<ValidatorSetV1>,
}

```

**Store Layer Composition**

The `CompositeStore` is a store defined in `forking-data-store` that routes each data capability to a store type. The building blocks come from the `forking-data-store` crate:

- `WriteThroughStore<Primary, Secondary>`: writes to both; reads from primary first, falls back to secondary, writes to secondary, and then writes to primary.
- `ReadThroughStore<Primary, Fallback>`: reads from primary, falls back to secondary and caches result into primary.

The concrete layers:

```rust
MemFs             = WriteThroughStore<InMemoryStore, FileSystemStore>
DiskThenGraphql   = ReadThroughStore<FileSystemStore, DataStore(GraphQL)>
HotObjects        = WriteThroughStore<InMemoryStore, DiskThenGraphql>

ForkDataStore    = CompositeStore<
transactions: MemFs,      // mem → fs (2 tiers)
epochs:       MemFs,      // mem → fs (2 tiers)
objects:      HotObjects, // mem → fs → GraphQL (3 tiers)
checkpoints:  MemFs,      // mem → fs (2 tiers)
>
```

Objects have a third tier (GraphQL) because they are fetched on-demand from the
live network when not found locally. Transactions, epochs, and checkpoints are
only produced locally (or fetched once at startup via gRPC), so two tiers suffice.

**Filesystem Layout**

The user has the option to specify where to store the network’s state through a `--data-dir` flag at startup. If none is provided, the default `~/.forking_store` will be used.

```bash
 <data_dir>/forking/<network>/forked_at_checkpoint_<N>
	 objects
	 checkpoints
	 transactions
```

Storing data on filesystem enables resuming a previously forked session — on restart, if the user provides a checkpoint to fork from that was previously used, the tool will resume from that state that should exist on the filesystem.
In the case that the user wants to fork from that checkpoint again on a fresh & clean state, the tool provides a `--reset` flag to remove that directory and start fresh.

### **Startup object seeding**

At startup, the user has the choice to seed addresses or objects, to make the forked network “aware” of them. There are two different modes on how this works, due to the data-limitations that we have in GraphQL:

- `--accounts`: discover owned objects through GraphQL at startup time for checkpoints not older than 1h.
- `--objects`: provide explicit object IDs to prefetch at startup time, required for checkpoints older than 1h.

note that these two args are mutually exclusive.

### Validators

In a forked network, we do not have access to the private keys of the real validators. The tool must overwrite the validator set at startup with a custom generated one from a new genesis. That way, when epoch changes happen, we can safely load these validators as we have their private keys.

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

- `-checkpoint <number>`: The checkpoint to fork from
- `-network <network>`: Network to fork from: `mainnet (default)`, `testnet`, `devnet`, or a custom one (`-network <CUSTOM-GRAPHQL-ENDPOINT> --fullnode-url <URL>` ). The latter is useful for “forking” from a custom local network  / private network. It requires to have a GraphQL service running and a fullnode as well.

**The startup flow**

1. Fetch latest checkpoint number via GraphQL (or use specified checkpoint)
2. Fetch protocol version via GraphQL
3. Compose the three-layer forking-data-store from filesystem + in-memory + GraphQL
4. Create a single-validator config
5. Load startup checkpoint and cache epoch transition outputs
6. Seed startup objects
7. Build initial system state with validator set override
8. Fetch system packages at the forked checkpoint time
9. Initialize simulacrum with the composed store, initial system state, startup checkpoint
10. Bind both gRPC and HTTP listeners

### CLI

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

**Advance Epoch**

```bash
sui-forking advance-epoch
```

Advances the epoch of the local network by 1.

**Check Status**

```bash
sui-forking status
```

Shows the current checkpoint, epoch, and timestamp.


