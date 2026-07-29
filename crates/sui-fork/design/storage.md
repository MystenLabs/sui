<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# `sui-fork` storage

A fork node executes transactions locally on top of a chain whose state it mostly does not
have. Its storage therefore answers two questions at once. *What has this fork written?* is
served from a stock `sui-rpc-store` RocksDB, the same schema and indexes a real RPC node
uses. *What did the forked-from chain look like?* is answered lazily, by querying GraphQL
pinned at the fork checkpoint and caching the result into that same database.

A read and a write each pass through the same small set of components:

```
get_object(id)                       (latest semantics: an RPC read on ForkStore)
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

The RocksDB instance, the `fork_metadata.json` check that a data directory belongs to the
network and fork checkpoint it claims, and the embedded indexer are all brought up by a
service manager and watched for the lifetime of the node. `ForkStore` is the
`SimulatorStore` surface Simulacrum executes against, and it owns both the split between
local-first reads and remote fallback and the sealing of checkpoints; it delegates row
access to `LocalStore` (object materialization, checkpoint and transaction persistence, the
latest-object-status lookup) and every GraphQL round-trip to `RemoteSource`. Those
round-trips are pinned at the fork checkpoint wherever the request allows it, so they cannot
see what the forked-from chain did afterwards; everything above the fork point is the fork's
own and comes from the local store.

## Where reads resolve

A fork is a chain that diverged from another at a checkpoint. It shares history at or below
the fork point, which belongs to both chains and which the fork can obtain on demand from
the forked-from chain, and the fork's own history above it, which exists nowhere else. Every
read is a question about that composite world, and three rules decide how it is answered.

Local knowledge, when it is authoritative, always wins. The real network is consulted
only for shared history, and only through queries pinned at the fork point.
Anything the forked network finalized *after* the fork point is never admitted, because
that history did not happen here.

What differs between reads is only how the request lets the fork decide which part of the
data it is asking about.

### Requests keyed by a checkpoint

The key already answers the question. A checkpoint above the fork point can only have been
produced by this fork, so it is served locally. A checkpoint at or below the fork point is
shared history, so a local miss is resolved from the live network RPC, pinned at that
checkpoint, and persisted locally.

Reads for the *latest* checkpoint ask a different question, and they always mean the fork's
own tip: the fork point itself until the fork has executed something, and above it
thereafter.

### Requests keyed by a digest (e.g., transactions, effects)

A digest carries no information about which side of the fork point it falls on, so the fork
cannot classify the request before answering it. Transactions, their effects, and the
checkpoint that finalized them all have this shape.

Local storage is tried first and is authoritative when present. In case of a miss, the
data is fetched from the live network RPC and checked that it did exist at the fork point or earlier.
Transactions executed locally will be found in the local store.

### Requests keyed by an object

There are three ways to ask about an object: by an exact version, with no version at all, or
bounded by a version. Each has different implications for how the fork can answer it.

- An exact version is an immutable key: a given version of an object never changes, so a local
row can be served without further thought, and a miss can be resolved from the live network RPC
pinned at the fork point.

- A request with no version asks what is *current*, which is a question about the fork's own
state rather than about history, and it is the one object read that cannot be answered from
stored object rows alone. The fork holds versions sparsely, caching whatever some earlier
read happened to need, so the highest stored version is not necessarily the live one, and
finding nothing stored cannot distinguish an object that was removed from one that was never
fetched. That distinction decides whether to consult the live network RPC at all, which is
why live state is tracked explicitly; the following section describes how.

- A request bounded by a version is the subtle one. It asks for the highest version at or
below some bound, which is how child objects are read during execution. A stored row at or
below the bound is only trustworthy if the fork knows nothing newer can exist below it, which
holds when that row carries live-state authority or is a tombstone. Absent that, the sparse
cache may hold some older version that an unrelated historical read left behind, and serving
it would be wrong, so the bound must also be resolved against the forked-from chain and the
higher of the two answers taken.

### Derived reads compose over policy

Some reads are not lookups at all but functions of other reads. A transaction's events are
stored with the transaction. A type's layout is a function of the packages it references,
and packages are objects. Coin metadata is a function of the `CoinMetadata`, `TreasuryCap`,
and `RegulatedCoinMetadata` objects belonging to a coin type.

Such reads must resolve their inputs through the fork's own read policy rather than by
reaching into stored rows directly. A layout resolved by loading packages straight from
local storage cannot render a pre-fork type whose package has never been materialized, even
though the object policy one layer down would have fetched it happily. The rule is that
composing a derived read out of raw storage discards the policy that makes its inputs
correct, so derived reads compose over the policy, never under it.

Coin metadata is worth naming here because it looks like an enumeration and is not. It
resolves three type-keyed lookups and composes them into one answer, so it belongs to this
section rather than to the inventories below: an inventory is complete over an owner, and
nothing enumerates the coin types a fork might be asked about, so there is no scan to take
in advance. Each of the three lookups resolves through the object policy when something asks.

Events are the same principle seen from the other side. They need no policy of their own
precisely because the transaction read that produces them has one: whatever pulled the
transaction in pulled its events with it. That holds only while every path to the events
reaches them through a transaction, which is a property of the callers rather than of this
crate, and one worth stating because nothing here enforces it.

### Requests keyed by nothing: indexes and enumeration

Reads that enumerate rather than look up split into two kinds.

A *state* index answers what is true as of a checkpoint: which objects an address owns,
which objects have a type, what an address's balance is, which versions of a package exist.
Because that is a question about state at a point in time, the forked-from chain can answer
it pinned at the fork point.

It answers it as a set of object references rather than as a set of objects, and keeping
those two apart is what makes the enumeration affordable. Establishing *which* objects an
owner held is the part that has to be complete and has to happen while the question is still
answerable: it is one query pinned at the fork point, and nothing the fork does afterwards
reconstructs it. Fetching what each of those objects *contains* is a different question, an
exact version and so an immutable key, and the object policy above already answers it on
demand. So the fork enumerates eagerly and materializes lazily: it takes the complete
reference set up front and records that it did so, and each object is hydrated the first time
something asks for it, through the same path any other version-keyed read would take.

Local execution maintains the answer forward from there, so later reads are purely local.
Completeness is the point, so a partial answer is worse than none: a scan that could not run
must never be recorded as one that ran and found nothing.

A *log-position* index answers what sits at a given position in the total order of every
transaction ever executed. That is not a function of state at any checkpoint, and no query
pinned anywhere can produce it. The fork inherits its position numbering from the fork point
and starts writing at the next position, so every position below is a real transaction on
the other chain that this fork does not hold and cannot obtain. The fork's ledger begins at
the fork point, and this is a permanent property of forking rather than a missing feature.

## The object live state

Object live state is recorded in `object_version_by_checkpoint`, which maps `(ObjectID, checkpoint)`
to the version the object ended that checkpoint at, and which the embedded indexer already
maintains for every checkpoint the fork executes. The owner and type indexes also record
latest live versions, but they are keyed by owner and type and cover only indexed objects,
so neither can be used to query for an arbitrary object id.

A live-state read is a floor scan over that index, bounded at the checkpoint the fork is
currently producing, and it has exactly the three outcomes the fork needs: a row at a live
version, a row at a tombstone version, which is authoritative and never falls back, and no
row at all, meaning no local knowledge, so ask the remote.

The index infers liveness: the object changed to *v* at checkpoint *C*, and no row exists
above *C*. That holds only where the fork's change history is complete, so what may write to
it is confined to where that is true. Locally executed checkpoints record ordinary rows
keyed by the checkpoint producing them, a range complete because the fork executed all of
it. Pre-fork materialization records a floor row at the fork checkpoint, the same shape a
live-set restore writes at its anchor, because a query pinned there establishes exactly
that: the object was live at the fork point, at that version. A version-keyed fetch records
nothing at all. It is evidence about one point in history and none about what is live, so
keying it at the fork checkpoint would falsely claim the version is current, while keying it
at the version's own creation checkpoint would assert an absence of later changes that a
sparse store cannot rule out.

Within one checkpoint's application, removals stage before writes, so an object wrapped
and re-created in the same result ends up live: both write the same checkpoint-pinned key
and the later put wins. Because the index shares a database with the rows it describes,
each object row and the live-state row covering it commit in a single batch.

## Executing and indexing

Everything canonical is written synchronously and everything derived is left to the indexer.
Simulacrum inserts the pieces of an in-flight checkpoint as it executes. These pieces are staged in
`PendingCheckpointBuffer` until the seal writes them out atomically to the DBs. Each
executed transaction writes its object version rows, tombstones, and checkpoint-pinned
version rows before execution proceeds, and sealing writes the checkpoint summary, contents, and every
transaction's data, effects, and events. These writes cannot wait: the executor needs
read-your-writes for the next transaction's inputs, and the indexer ingests each sealed
checkpoint by reading it back out of the same rows.

The derived indexes (owner, type, package-version, balance, bitmaps) are written for
local checkpoints by the embedded indexer alone, which runs every stock pipeline starting
one checkpoint after the fork point; the fork gets the full derived-index surface without
maintaining any of it. Sealing and publication are serialized through `Context`'s
publication lock, and publication blocks on the minimum watermark across every pipeline the
stock layer enables, so by the time an execution returns to its caller the checkpoint is
fully indexed, and any RPC read issued afterwards sees complete derived state. Subscribers
receive checkpoints from the indexer's broadcast pipeline, so their ordering is inherited
from indexing rather than from sealing.

Pre-fork state is the one exception, because it never flows through the indexer at all.
When a seed, an inventory, or a lazy materialization brings a pre-fork object in, its
derived rows are written synchronously alongside it: the saves that hydrate a pre-fork
object write the owner, type, package, and balance rows, and lazy materialization writes the
package-version row for fetched packages. This does not create a second writer for any
row: those saves cover versions at or before the fork checkpoint, a range the indexer
never touches.

The `SimulatorStore` write surface cannot return errors, so a failed persist panics rather
than letting execution continue on state that has diverged from disk. An indexer stoppage
is likewise surfaced the moment it happens, because the startup loop watches for it as a
liveness watchdog, instead of appearing later as a publication timeout.

## Seeding and inventories

An **inventory** is the one-time, complete enumeration of what an owner held at the fork
checkpoint, per address owner or per object owner. It runs at startup, and it stores the
reference set rather than the objects: the completion marker recorded in
`inventory_metadata.json` means the fork knows exactly which ids that owner held there, and
each object is materialized on first use through the ordinary version-keyed read. Once the
marker exists, owner-scoped reads are served locally.

Startup is not an incidental choice. The enumeration is the one part that cannot be
recovered later, and running it before any read can depend on it is what keeps a failure
attributable: the fork either has a complete reference set or knows it does not, and never
has to decide mid-request what an empty answer meant.

Seeding (`--address`, `--object-id`) is the same mechanism made explicit. An address seed is
an inventory named on the command line, so it enumerates, records the address in an
immutable manifest, and marks its inventory complete. An address that owns nothing at the
fork checkpoint is authoritatively empty and is marked as well, because that is a complete
answer and not a missing one. Explicit object-id seeds mark nothing, because naming objects
is not a complete enumeration of any owner.

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
same checkpoint number, so those orphaned rows still fall within a live-state read's bound
and are served as though the checkpoint had sealed. This is the main known gap.

Type layout resolution reaches under the object policy rather than composing over it. It
loads the packages a type references straight from stored rows, so a pre-fork type whose
package has never been materialized resolves to no layout at all, even though an object
read for that same package would have fetched it. Every read that renders Move values as
JSON depends on this. The same shortcut also picks packages by scanning stored versions,
which is safe for ordinary packages, immutable at one version per id, but not for system
packages, which carry every version they have ever had under one id.

An enumeration that could not run is still recorded as one that ran and found nothing. The
forked-from chain retains only a window of history, and a fork point below that window
cannot be enumerated at all. Taking enumerations at startup is what makes this detectable,
since the fork learns the window's floor and can compare it against its own fork point
before recording anything, but that comparison is not yet made for every enumeration, so a
scan attempted below the window returns empty, is marked complete, and that emptiness then
answers every later read for that owner.

Address balances held in the accumulator, as opposed to in coin objects, are neither
seeded nor served. The balance index reflects only coin objects materialized pre-fork plus
what the indexer derives post-fork.

`simulate_transaction` is stubbed; there is no Simulacrum entrypoint for it yet. It is
planned as a follow-up.

Bounded child reads can serve stale history. `get_object_lt_or_eq_version` trusts the
highest *local* row at or below the bound, so a sparse cache polluted by an
exact-historical-version read, an RPC client fetching an old dynamic-field version say, can
hold a row lower than the true highest-≤-bound, which then wins without the remote ever
being consulted. This affects `read_child_object` on both the RPC and executor paths, so it
can skew execution and not only reads. Closing it means short-circuiting only on a live-state
row or a tombstone, and otherwise merging the remote `RootVersion(bound)` result with the
local candidate by maximum version.

The fork's ledger index begins at the fork point, and the range below it is permanently
empty. Positions are inherited rather than restarted: Simulacrum is seeded with the real
fork checkpoint, whose summary carries the chain's `network_total_transactions`, and each
transaction's position is derived from it, so the fork's transactions continue the real
chain's numbering. Every position below that value therefore names a real transaction the
fork's index does not hold. The ledger tx-seq family is a log-position index, and the two
bitmap families bucket over the same coordinate.

The visible effects are `ledger_tx_seq_digest` returning nothing for any pre-fork position,
and `ListTransactions` and `ListEvents` returning an empty stream rather than an error for
a pre-fork checkpoint range, which a client reads as "nothing matched" rather than "not
available here." Filtered scans inherit the same limit, since filters are evaluated against
the bitmaps.

Positional lookups cannot be closed by falling back to the remote. GraphQL exposes
transactions by digest, by checkpoint, and by filter, never by global ordinal, so deriving
what sits at a position would mean counting every transaction from genesis. The range
queries are closable in principle, since they are parameterized by checkpoint range and
filter and a checkpoint-pinned remote query can express both, but at a cost that scales with
the range rather than with the result, and the filter would have to be delegated to the
remote rather than evaluated against local bitmaps. Neither is planned. The fork's ledger
starts at the fork point.
