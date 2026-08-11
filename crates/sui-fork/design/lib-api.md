<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# `sui-fork` as a library

A fork node can be started from inside another Rust program through one call:

```rust
let fork = ForkNode::start(args, version, registry).await?;
```

`StartArgs` says what to fork and where to serve it; the returned `ForkNode` is the running
fork — it reports what was started, drives the fork's clock and checkpoints in-process, and
controls when the fork stops. The crate's own binary is the first consumer; the intended one
is the Sui CLI, which would start a fork the same way it already starts its faucet, indexer,
and GraphQL services. Storage, execution, and read routing are unchanged by this API and are
argued in [storage.md](storage.md); this document covers only how a program configures,
starts, observes, and stops a fork.

## What `start` does

```rust
#[derive(clap::Args, Clone, Debug)]
pub struct StartArgs {
    /// Network to fork from: mainnet, testnet, devnet, or a custom GraphQL URL.
    #[arg(long, default_value_t = Self::default().network)]
    pub network: Node,
    /// Checkpoint to fork at; `None` resumes an existing fork or forks at latest.
    #[arg(long)]
    pub checkpoint: Option<CheckpointSequenceNumber>,
    /// Root directory for fork state; `None` uses the default data root.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// Addresses whose owned objects are seeded (fresh forks only). Repeatable.
    #[arg(long = "address")]
    pub addresses: Vec<SuiAddress>,
    /// Objects to seed when address-owned (fresh forks only). Repeatable.
    #[arg(long = "object")]
    pub object_ids: Vec<ObjectID>,
    /// Address the gRPC server binds; port 0 selects an ephemeral port.
    #[arg(long = "rpc-addr", default_value_t = Self::default().rpc_listen_address)]
    pub rpc_listen_address: SocketAddr,
}

impl ForkNode {
    pub async fn start(
        args: StartArgs,
        version: &'static str,
        registry: &prometheus::Registry,
    ) -> Result<ForkNode>;
}
```

`start` performs the whole startup sequence, and returns once the server
is accepting connections:

```
ForkNode::start(args, version, registry)
  ├─ bind the listener            (first, before anything durable exists: a taken port —
  │                                the most likely failure — errors out while a retry can
  │                                still start from a clean slate)
  ├─ resolve the fork point       (local fork state if inspectable, else the requested
  │                                checkpoint, else the network's latest)
  ├─ open or create the data dir  (fork_metadata.json validated, rpc-store RocksDB opened)
  ├─ seed                         (fresh forks only; a resumed fork's manifest is replayed
  │                                as a no-op — see storage.md, "Seeding")
  ├─ build the executor           (Simulacrum over ForkStore, local validator keys)
  ├─ start the embedded indexer   (a sui-futures Service, merged into the fork's own)
  └─ serve                        (over the already-bound listener, so the reported
                                   address is the true one even when port 0 was asked for)
```

Fork-point resolution sits behind `start` because every caller wants the same policy and
answering it takes both local inspection and a remote query. An explicit checkpoint forks
there, resuming if that fork's directory already holds state. With no checkpoint, `start`
resumes the fork found in the configured data directory and otherwise forks at the network's
latest checkpoint. A directory can only be inspected when the caller names it, or names the
checkpoint from which the default path is derived: the default layout keys each fork's
directory by network and checkpoint, so with neither given there is nothing to probe and
latest is the answer. This policy previously lived in the binary's command layer, which is
exactly the duplication every embedder would have been forced into.

`start` installs no signal handlers and initializes no tracing. Binaries own both across
this repository, and a library that claims SIGINT or the global subscriber cannot be
embedded next to anything else that does.

## The shape of the entry point

The arguments are a plain struct with public fields, a `Default` (mainnet, latest, default
data root, no seeds, `127.0.0.1:9000` — the fullnode RPC default, on purpose: the fork
stands in for a fullnode, so clients that default to `localhost:9000` reach it unchanged,
and running next to a real fullnode is the exception that picks another port and fails
loudly at bind time otherwise), and a `clap::Args` derive whose flag defaults are
read from that `Default` — one struct serves the command line and the programmatic caller,
and the two sets of defaults cannot drift. The services `sui start` composes take their
options the same way (the indexer's and consistent store's args derive both), while the
builder pattern lives in the older swarm and test-cluster tier, whose handles predate the
shared service abstraction. Six data fields do not need staged construction, and following
the current pattern keeps a future `sui start --fork` symmetrical with the services it would
sit beside: it flattens the same struct into its own command line, exactly as the `sui-fork`
binary's start command does.

`version` and `registry` arrive as arguments of `start` rather than as config fields because
they describe the embedding binary rather than the fork. The version must be `&'static str`
— the RPC layer's server-version type requires it, and the macro that mints version strings
compiles only in binaries — so the embedder supplies its own. The registry is threaded into
the subscription service, the embedded indexer, and the RPC service's metrics, letting the
host scrape the fork alongside its other services. The startup path previously constructed
a registry internally and dropped it, so the fork served no metrics at all; taking the
registry as a parameter closes that gap. Prometheus rejects duplicate collector
registration, so each fork needs its own registry.

`start` is one call rather than the initialize-then-serve pair the binary used before it.
That split existed so the binary could print its startup summary between the phases,
forcing the summary to show the requested listen address — wrong the moment port 0 is allowed. With one
call the facts are read off the handle after the bind, so what is printed is what was bound.
The stock serving entry in `sui-rpc-api` takes an address, panics if the bind fails, and
never reports the bound socket; the service also exposes its assembled router, so `start`
binds its own listener, returns bind failures as errors, and serves the router with a
shutdown wired to the handle. The crate's tests used to probe a free port and race to
rebind it; serving this way removed that race. The bind also comes first, before the data
directory, the seed manifest, or any metric registration exists, because startup's side
effects are not idempotent — prometheus panics on re-registered collectors and a written
seed manifest refuses new seed flags — so the failure most likely to be environmental must
fire before the side effects that would poison the retry. The cost is that the bound port
accepts TCP connections for the whole of initialization: a client that connects early waits
in the accept backlog instead of being refused, so a successful connect is not readiness and
clients should carry request deadlines.

## The handle

`ForkNode` answers three kinds of question. What was started: the bound address, the network
name, the resolved data directory, the fork checkpoint, the checkpoint the fork's local tip
was at when `start` returned, and whether the fork was resumed — the facts the binary
prints. How to drive it:

```rust
pub async fn advance_clock(&self, duration: Duration) -> Result<ClockAdvanced>;
pub async fn create_checkpoint(&self) -> Result<CreatedCheckpoint>;
pub async fn status(&self) -> Result<ForkStatus>;
```

These share one implementation with the forking gRPC service — the service holds the same
`ForkAdmin` handle the node hands out — so the in-process and remote surfaces cannot drift
apart, and a host that just started a fork can advance its clock without dialing the socket
it opened.

Checkpoint-producing calls succeed once the checkpoint is sealed, which happens before the
indexing wait. An indexer that misses the wait therefore cannot fail the call — reporting
failure for a durable state change would invite retries that execute twice, the worse
outcome by far — and it cannot panic the host either, which `panic=abort` release builds
would turn into process death. The miss is logged as an error and the fork limps: derived
reads lag raw reads and subscribers are not notified until the indexer catches up. No
background watchdog polices this. The fork is a command-driven sequential node — one user
transaction, then its checkpoint — so every indexing cycle that matters is already observed
by the call that produced it, and a watchdog would only add false positives on an idle fork.
The checkpoint-producing closure runs inline on the calling task, blocking it for the
duration of Move execution and any synchronous live-network reads. That stall is accepted
for the same sequential-node reason, and because the RPC read paths make the same
synchronous remote reads on their own tasks — dispatching only the executor to a blocking
thread would not remove the class.

`ForkNode::admin` returns a cheap cloneable handle carrying the same three methods. It
exists because the two ways a host holds a running fork both make the methods on the node
itself unreachable: `into_service` consumes the node, and `join` borrows it mutably for as
long as the watch lasts. A host takes the admin handle first and keeps driving the fork
either way; the handle does not keep the fork alive.

The third question is the fork's lifetime. Underneath, the fork's tasks — the server and
the embedded indexer — live in one `sui-futures` `Service`, the abstraction every off-chain
service in `sui start` returns. `join` resolves when a task fails or panics, taking over
the watchdog role a hand-rolled select loop used to play. A wedged indexer is the one
failure `join` does not see, because the indexer framework retries its own failures
indefinitely; it surfaces as the logged stalls described above. `shutdown` stops the server and winds down the indexer.
`into_service` surrenders the tasks for composition — a host merges the fork with the other
services it manages and keeps ownership of its own signals. The gRPC surface at the bound
address remains, and a `ForkNode::admin` handle taken beforehand keeps the in-process admin
methods.

## Stopping

Dropping the handle aborts the fork's tasks, which is the right behavior for a test that
panicked; every deliberate exit goes through `shutdown` or a service main loop. A graceful
stop drains the server and lets the indexer finish the checkpoint it is publishing.

Stopping does not seal. Transactions executed but not yet gathered into a checkpoint are
lost on stop, exactly as they are lost when the process exits. Sealing at shutdown
would create a checkpoint the user never asked for, making the act of stopping a fork change
that fork's state; a caller that wants its pending transactions to survive calls
`create_checkpoint` before `shutdown`. Everything sealed is durable by the database's own
guarantees.

## What stays private

The executor handle is the largest omission. Exposing the simulacrum would put `ForkStore`
and the executor's locking discipline into the public surface, and its simulation path
blocks the calling thread with Move execution and synchronous remote reads. An embedder
holding the raw lock inherits that trap along with every internal type it can reach.
In-process execution and simulation are left out for the same reason: the gRPC execution
path already exists and carries the sender-impersonation rules.

The rest of the crate narrows to match. The GraphQL client, the startup internals, the seed
input, and the store become crate-private; the public surface is `StartArgs`, `ForkNode` and
its return types, `Node`, the command-line types, and the forking-service client types a
host binary needs for client-side subcommands against an already-running fork. The crate was
not a workspace dependency before this change, so nothing outside it could name any of the
previous wider surface, and narrowing cost nothing; once consumers exist every demotion is a
breaking change.

Errors are `anyhow` throughout. The crate is unpublished and already uses `anyhow`
everywhere, and a typed error enum would add a contract with no consumer that can act on the
distinctions; callers that need to react to failure modes programmatically have the gRPC
status codes.

## The binary on top

The crate's start command becomes the API's first consumer: build a registry, call `start`,
print the facts off the handle, convert the handle into its `Service`, and run the standard
service main loop. The printed address is the bound one, and the loop waits on both
SIGINT and SIGTERM before shutting down gracefully, where the loop it replaced recognized
only Ctrl+C and exited without winding anything down. A Sui CLI command does the same up to the
last step, merging the fork's `Service` with the other services it manages instead of
running the fork's alone.

The crate's test harnesses, which assemble store, executor, and server by hand, become the
second consumer through a crate-private constructor that serves a pre-built context: the
tests exercise the serving half of the API against synthetic state, and the binary exercises
the whole of it against a live network.
