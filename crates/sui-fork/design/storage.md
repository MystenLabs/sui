<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# `sui-fork` storage

A fork node executes transactions locally on top of a chain whose state it mostly does not
have. Its storage therefore answers two questions at once: *what has this fork written?* —
served from a stock `sui-rpc-store` RocksDB, the same schema and indexes a real RPC node
uses — and *what did the forked-from chain look like?* — answered lazily, by querying
GraphQL pinned at the fork checkpoint and caching the result into that same database.
Nothing upstream changes to make this work: `sui-rpc-store` and `sui-rpc-node` are
untouched, everything fork-specific lives in this crate, and gRPC is served directly
through `sui-rpc-api`'s `RpcService` rather than through `sui-rpc-node`.

A read and a write each pass through the same small set of components:

```
get_object(id)                       (latest semantics — ForkRpcReader → ForkStore)
  ├─ consult the pointer table       (LiveState: Live(v) | Removed | absent)
  ├─ Live(v):  objects[(id, v)]      (LocalStore, a stock rpc-store row)
  ├─ Removed:  not found             (authoritative tombstone, no fallback)
  └─ absent:   fetch from GraphQL    (RemoteSource, pinned at the fork checkpoint;
                                      the row is persisted, then the pointer set)

execute(tx)                          (Simulacrum, with ForkStore as its SimulatorStore)
  ├─ stage the outputs               (PendingCheckpointBuffer, in memory)
  ├─ write rows and pointers         (synchronous: object versions, tombstones, LiveState)
  ├─ seal the checkpoint             (summary, contents, per-tx data/effects/events)
  ├─ index it                        (embedded rpc-store Indexer, every stock pipeline)
  └─ publish                         (blocks until every pipeline has caught up)
```

The pieces the diagram names each have one job. `ForkRuntime` owns everything that must
exist before any of it can run: the two RocksDB instances, the `fork_metadata.json` check
that a data directory belongs to the network and fork checkpoint it claims, and the
embedded indexer, started with the runtime and watched for the lifetime of the node.
`ForkStore` orchestrates the rest — local-first reads with remote fallback, checkpoint
sealing, and the `SimulatorStore` surface Simulacrum executes against — delegating row
access to `LocalStore` (object materialization, checkpoint and transaction persistence,
the latest-object-status lookup) and every GraphQL round-trip to `RemoteSource`. All
remote-read policy lives in that one place: queries pinned at the fork checkpoint, the
gates that refuse to ask the remote about post-fork checkpoints and transactions, and
validation of the references a response carries.

## Where reads resolve

`ForkRpcReader` implements the upstream RPC storage traits and is deliberately thin:
every read the fork has policy for resolves through `ForkStore`, which is already
local-first — it checks the rpc-store rows before consulting the remote — so a second
routing layer in the reader would only repeat the same point-get. The reader touches the
stock reader directly only for surfaces the fork keeps no policy for at all — events,
full checkpoint contents, committees, epoch info, struct layouts, and the ledger and
bitmap indexes, all written by the embedded indexer and correct to serve as-is — plus two
genuinely hybrid reads: the chain identifier (the framework table seeded at open, derived
from the fork checkpoint when absent) and the highest indexed checkpoint (the indexer
watermark, with the highest persisted checkpoint standing in before the first watermark
is written).

Latest-semantics reads are why the routing must go through `ForkStore`. The stock reader
answers `get_object` without a version by reverse-scanning the `objects` column family,
which is only correct when the version history is complete. The fork's history is sparse:
a historical version is present only because something once fetched it. Serving the
highest cached row as "current" would be silently stale, so latest reads consult the
`LiveState` pointer instead.

## `LiveState`: the current-version authority

Nothing in the stock schema can tell the fork "this object's current version is *v*, and
it is live" — or "it was removed." `object_by_owner` and `object_by_type` do record latest
live versions, but they are keyed by owner and type and cover only indexed objects. The
`objects` family is keyed by `(id, version)`, and because the fork's copy is sparse, a
reverse scan that finds nothing cannot distinguish *removed* from *never cached* — which
is exactly the distinction that decides whether to fall back to the remote chain.
`LiveState`, a fork-owned single-column-family RocksDB, records that distinction per
`ObjectID`:

- `Live(version)` — read `objects[(id, version)]` locally; never fall back.
- `Removed { version, kind }` — an authoritative tombstone; never fall back.
- absent — no local knowledge; ask the remote.

Two write orderings keep this fail-safe. Rpc-store rows commit *before* the pointer that
makes them authoritative, so a reader racing the update can transiently miss the pointer —
which degrades to "unknown" and a redundant remote fetch, never to a wrong answer. And
within one checkpoint's application, removals stage *before* writes, so an object wrapped and
re-created in the same result lands `Live` rather than tombstoned.

## Executing and indexing

Everything canonical is written synchronously; everything derived is left to the indexer.
Simulacrum inserts the pieces of an in-flight checkpoint as it executes; they stage in the
`PendingCheckpointBuffer` until the seal writes them out atomically. Each
executed transaction writes its object version rows, tombstones, and `LiveState` pointers
before execution proceeds, and sealing writes the checkpoint summary, contents, and every
transaction's data, effects, and events. These writes cannot wait: the executor needs
read-your-writes for the next transaction's inputs, and the indexer ingests each sealed
checkpoint by reading it back out of the same rows.

The derived indexes — owner, type, package-version, balance, bitmaps — are written for
local checkpoints by the embedded indexer alone, which runs every stock pipeline starting
one checkpoint after the fork point; the fork gets the full derived-index surface without
maintaining any of it. Sealing and publication are serialized through `Context`'s
publication lock, and publication blocks on the minimum watermark across all seventeen
pipelines, so by the time an execution returns to its caller the checkpoint is fully
indexed, and any RPC read issued afterwards sees complete derived state. Subscribers
receive checkpoints from the indexer's broadcast pipeline, so their ordering is inherited
from indexing rather than from sealing.

Pre-fork state is the one exception, because it never flows through the indexer at all.
When a seed, an inventory scan, or a lazy materialization brings a pre-fork object in, its
derived rows are written synchronously alongside it: seed and inventory saves write the
owner, type, package, and balance rows, and lazy materialization writes the
package-version row for fetched packages. This does not create a second writer for any
row: those saves cover versions at or before the fork checkpoint, a range the indexer
never touches.

The `SimulatorStore` write surface cannot return errors, so a failed persist panics rather
than letting execution continue on state that has diverged from disk. An indexer stoppage
is likewise surfaced the moment it happens — the startup loop watches for it as a liveness
watchdog — instead of appearing later as a publication timeout.

## Seeding and inventories

An **inventory** is a one-time, complete remote enumeration — per address owner, per
object owner, or per type — taken at the fork checkpoint. It backfills the stock index
families and records a completion marker in `inventory_metadata.json`; once the marker
exists, owner-scoped reads are served locally. Inventories run lazily: the first read that
needs one triggers the `InventoryInitializer` scan, serialized under the snapshot lock it
shares with local writes.

Seeding (`--address`, `--object-id`) resolves an immutable manifest at startup. An address
seed performs the same complete scan an inventory would, so the manifest records those
addresses and, once every entry is saved, marks their inventories complete rather than
leaving a later read to repeat the enumeration. An address that owns nothing at the
fork checkpoint is authoritatively empty and is marked as well. Explicit object-id seeds
never mark their owners, because fetching named objects is not a complete scan of
anything. Manifests written before the `addresses` field existed carry no such record and
fall back to lazy initialization.

## Data-dir layout

```
{root}/
  fork_metadata.json        network + fork checkpoint + chain id (validated on open)
  seed_manifest.json        immutable seed record (exclusive create)
  inventory_metadata.json   completion markers for inventory scans (temp+rename)
  rpc_store/                stock sui-rpc-store RocksDB (RpcStoreSchema)
  live_state/               fork-owned RocksDB (single CF fork_live_state)
```

## Known gaps

The pending checkpoint buffer is memory only, so a crash mid-publication loses the
unsealed checkpoint and its transactions while their object rows and live pointers
persist. There is no startup reconciliation yet between `live_state` and the highest
sealed checkpoint; this is the main known gap, and it has a fail-open corner: a crash
inside the small window between a row commit and its pointer update leaves a locally
written object pointer-less, and a later read would re-resolve it from pre-fork GraphQL.

The rpc-store and `live_state` are separate RocksDB instances. Each commit is atomic
within its own database but nothing is atomic across the two; the write orderings above
are what make the inconsistency windows fail-safe rather than fail-open. The split is
historical rather than forced: `sui-consistent-store`'s `Schema` trait composes publicly,
so a fork-owned schema could host the live-state column family in the main database and
make row and pointer commits atomic — a candidate follow-up that would retire both the
orderings and the reconciliation gap above.

Address balances held in the accumulator, as opposed to in coin objects, are neither
seeded nor served. The balance index reflects only coin objects materialized pre-fork plus
what the indexer derives post-fork.

`simulate_transaction` is stubbed; there is no Simulacrum entrypoint for it yet.

Bounded child reads can serve stale history. `get_object_lt_or_eq_version` trusts the
highest *local* row at or below the bound, but the sparse cache can be polluted by an
exact-historical-version read — an RPC client fetching an old dynamic-field version, say —
leaving a row lower than the true highest-≤-bound, which then wins without the remote ever
being consulted. This affects `read_child_object` on both the RPC and executor paths. The
fix direction is to short-circuit only on live-state authority or an authoritative
tombstone, and otherwise merge the remote `RootVersion(bound)` result with the local
candidate by maximum version.
