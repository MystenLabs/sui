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

A read and a write each pass through the same small set of components:

```
get_object(id)                       (latest semantics — an RPC read on ForkStore)
  ├─ consult the version index       (object_version_by_checkpoint: live | tombstone | absent)
  ├─ Live(v):  objects[(id, v)]      (LocalStore, a stock rpc-store row)
  ├─ Removed:  not found             (authoritative tombstone, no fallback)
  └─ absent:   fetch from GraphQL    (RemoteSource, pinned at the fork checkpoint;
                                      the row and its index entry are persisted together)

execute(tx)                          (Simulacrum, with ForkStore as its SimulatorStore)
  ├─ stage the outputs               (PendingCheckpointBuffer, in memory)
  ├─ write the rows                  (synchronous, one batch: object versions,
  │                                  tombstones, checkpoint-pinned versions)
  ├─ seal the checkpoint             (summary, contents, per-tx data/effects/events)
  ├─ index it                        (embedded rpc-store Indexer, every stock pipeline)
  └─ publish                         (blocks until every pipeline has caught up)
```

In the above diagram, the RocksDB instance, the `fork_metadata.json` check
that a data directory belongs to the network and fork checkpoint it claims, and the
embedded indexer are handled by a service manager and watched for the lifetime of the node.
`ForkStore` orchestrates the split between local-first reads and remote fallback, checkpoint
sealing. The `SimulatorStore` surface Simulacrum executes against — delegating row
access to `LocalStore` (object materialization, checkpoint and transaction persistence,
the latest-object-status lookup) and every GraphQL round-trip to `RemoteSource`.
Queries are pinned at the fork checkpoint and will ignore fetching post-fork data from
GraphQL RPC. Any post-fork data will go through the local store.

## Where reads resolve

`ForkStore` implements the upstream RPC storage traits (`ReadStore`, `RpcStateReader`,
`RpcIndexes`) itself. Startup hands the store to `RpcService` directly.
The impls are a thin layer. Every read the fork has policy for resolving through the store's
inherent helpers, which are already local-first. They check the rpc-store RocksDB rows 
before consulting the remote RPC. The impls touch the stock reader directly only for
surfaces the fork keeps no policy for at all, e.g., events, full checkpoint contents,
committees, epoch info, struct layouts, and the ledger and bitmap indexes, all written by
the embedded indexer and correct to serve as-is. For events, see * below.

There are two other reads that the fork does not resolve through the stock reader: the
chain identifier (the framework table seeded at open, derived from the fork checkpoint
when absent) and the highest indexed checkpoint (the indexer watermark, with the highest
persisted checkpoint standing in before the first watermark is written).

*Events are the one entry in that list whose correctness is borrowed rather than intrinsic.
The read in question is the `ReadStore` trait method, not an endpoint. No gRPC route
fetches events by digest, and clients reach them either through a transaction read or
through `ListEvents`. The fork's impl reads the rpc-store and stops there, yet a pre-fork
transaction's events are in that store only because the transaction fallback pulled them in
alongside the transaction row.

A transaction read is safe by ordering. `sui-rpc-api` resolves a transaction, then its
effects, then its events, and does so unconditionally, so the fork's transaction policy
has always run by the time the events read happens. `ListEvents` gets no such help: the fork does not override `multi_get_events`,
whose trait default maps over the same per-digest read, so it arrives with no transaction
read in front of it. It is safe instead because of what it can name. Its cursor comes from
the event bitmap and ledger tx-seq index families, which the embedded indexer writes only
for the checkpoints the fork executed, so every digest it can produce belongs to a locally
executed transaction whose events are already on disk. That is the same property recorded
under "Known gaps" as the reason pre-fork history cannot be enumerated.

Neither guarantee is enforced from this crate, and the exposure is wider than a change in
resolution order. Any caller that reaches the trait method with a pre-fork digest it has
not first resolved as a transaction — an events-by-digest endpoint added upstream, or a
scan whose cursor is not indexer-written — would see those transactions report *no* events
rather than missing ones, a wrong answer shaped like a valid one.

Latest-semantics reads are why that routing exists at all. The stock reader
answers `get_object` without a version by reverse-scanning the `objects` column family,
which is only correct when the version history is complete. The fork's history is sparse:
a historical version is present only because something once fetched it. Serving the
highest cached row as "current" would be silently stale, so latest reads resolve through
the checkpoint-pinned version index instead.

## The current-version authority

The `objects` family is keyed by `(id, version)`, and because the fork's copy is sparse, a
reverse scan that finds nothing cannot distinguish *removed* from *never cached* — which
is exactly the distinction that decides whether to fall back to the remote chain.
`object_by_owner` and `object_by_type` do record latest live versions, but they are keyed
by owner and type and cover only indexed objects. What answers the question is
`object_version_by_checkpoint`, which maps `(ObjectID, checkpoint)` to the version the
object ended that checkpoint at, and which the embedded indexer already maintains for
every checkpoint the fork executes. A currency read is a floor scan over it, bounded at
the checkpoint the fork is currently producing, and its three outcomes are the three the
fork needs: a row at a live version, a row at a tombstone version (never fall back), and
no row at all (no local knowledge, ask the remote).

That index infers liveness — "the object changed to *v* at checkpoint *C*, and no row
exists above *C*" — which holds only where the fork's change history is complete. Three
write rules confine it to where that is true. Pre-fork materialization records a
`from_restore` floor row at the fork checkpoint, the same shape a live-set restore writes
at its anchor, because a remote query pinned at the fork checkpoint establishes exactly
that: the object was live there, at that version. Locally executed checkpoints record
ordinary rows keyed by the checkpoint producing them, a range that is complete because the
fork executed all of it. And a version-keyed fetch records nothing at all: it is evidence
about one point in history and none about what is live, so keying it at the fork
checkpoint would falsely claim currency, while keying it at the version's creation
checkpoint would assert the absence of later changes that a sparse store cannot rule out.

Within one checkpoint's application, removals stage before writes, so an object wrapped
and re-created in the same result ends up live: both write the same checkpoint-pinned key
and the later put wins. Because the index shares a database with the rows it describes,
each object row and the authority making it current commit in a single batch.

## Executing and indexing

Everything canonical is written synchronously and everything derived is left to the indexer.
Simulacrum inserts the pieces of an in-flight checkpoint as it executes. These pieces are staged in
`PendingCheckpointBuffer` until the seal writes them out atomically to the DBs. Each
executed transaction writes its object version rows, tombstones, and checkpoint-pinned
version rows before execution proceeds, and sealing writes the checkpoint summary, contents, and every
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
```

## Known gaps

The pending checkpoint buffer is memory only, so a crash mid-publication loses the
unsealed checkpoint and its transactions, while the object rows and version-index entries
that checkpoint had already written persist. On restart the fork resumes producing the
same checkpoint number, so those orphaned rows still fall within a currency read's bound
and are served as though the checkpoint had sealed. This is the main known gap.

Address balances held in the accumulator, as opposed to in coin objects, are neither
seeded nor served. The balance index reflects only coin objects materialized pre-fork plus
what the indexer derives post-fork.

`simulate_transaction` is stubbed; there is no Simulacrum entrypoint for it yet. This
is done in a follow-up PR.

Bounded child reads can serve stale history. `get_object_lt_or_eq_version` trusts the
highest *local* row at or below the bound, but the sparse cache can be polluted by an
exact-historical-version read — an RPC client fetching an old dynamic-field version, say —
leaving a row lower than the true highest-≤-bound, which then wins without the remote ever
being consulted. This affects `read_child_object` on both the RPC and executor paths. The
fix direction is to short-circuit only on an authoritative current-version row or a
tombstone, and otherwise merge the remote `RootVersion(bound)` result with the local
candidate by maximum version.

The fork's ledger index begins at the fork point, and the range below it is permanently
empty. This is a different kind of limit from everything else here, so it is worth
separating two kinds of index. A *state* index — owner, type, coin, balance, dynamic
fields, package versions — answers "what is true as of checkpoint C", which a remote query
pinned at C can answer, which is why seeding and inventories work. A *log-position* index
answers "what sits at position K in the total order of every transaction ever executed",
which is not a function of state at any checkpoint. The ledger tx-seq family is the second
kind, and the two bitmap families bucket over the same coordinate.

Positions are inherited rather than restarted. Simulacrum is seeded with the real fork
checkpoint, whose summary carries the chain's `network_total_transactions`, and each
transaction's position is derived from it, so the fork's transactions continue the real
chain's numbering. Every position below that value therefore names a real transaction the
fork's index does not hold.

The visible effects are `ledger_tx_seq_digest` returning nothing for any pre-fork position,
and `ListTransactions` and `ListEvents` returning an empty stream rather than an error for
a pre-fork checkpoint range — silence a client reads as "nothing matched" rather than "not
available here." Filtered scans inherit the same limit, since filters are evaluated against
the bitmaps.

Positional lookups cannot be closed by falling back to the remote. GraphQL exposes
transactions by digest, by checkpoint, and by filter, never by global ordinal, so deriving
what sits at a position would mean counting every transaction from genesis. The range
queries are closable in principle — they are parameterized by checkpoint range and filter,
both of which a checkpoint-pinned remote query can express — but at a cost that scales with
the range rather than with the result, and the filter would have to be delegated to the
remote rather than evaluated against local bitmaps. Neither is planned. The fork's ledger
starts at the fork point.
