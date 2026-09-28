<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# `sui-fork` storage

A fork node executes transactions locally on top of a chain whose state it mostly does not
have. Its storage therefore answers two questions at once. *What has this fork written?* is
served from a stock `sui-rpc-store` RocksDB, the same schema and indexes a real RPC node
uses. *What did the live network look like?* is answered by querying GraphQL pinned at
the fork checkpoint and caching the result into that same database.

When that second question gets asked splits the design in two. A request that carries a key —
an object id, a version, a digest — can be answered whenever it arrives, because the key is
still valid later; so those are resolved on demand and cached. A request that carries no key
but asks for a *set* — which objects an address owns, which have a type — cannot, because
the enumeration that answers it is only available for a window after the fork point and
nothing reconstructs it afterwards. Those are settled once, at fork creation, by seeding.
Everything below follows from that split.

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
see what the live network did afterwards. Everything above the fork point is the fork's
own and comes from the local store.

## The object live state

Object live state is recorded in `object_version_by_checkpoint`, which maps `(ObjectID, checkpoint)`
to the version the object ended that checkpoint at, and which the embedded indexer already
maintains for every checkpoint the fork executes. The owner and type indexes also record
latest live versions, but they are keyed by owner and type and cover only indexed objects,
so neither can be used to query for an arbitrary object id at the latest version.

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
nothing at all because it's a one point in history and not about what is live, so
keying it at the fork checkpoint would falsely claim the version is current, while keying it
at the version's own creation checkpoint would assert an absence of later changes that a
sparse store cannot rule out.

Within one checkpoint's application, removals stage before writes, so an object wrapped
and re-created in the same result ends up live: both write the same checkpoint-pinned key
and the later put wins. Because the index shares a database with the rows it describes,
each object row and the live-state row covering it commit in a single batch.

## Executing and indexing

Everything canonical commits at the seal and everything derived is left to the indexer.
Simulacrum inserts the pieces of an in-flight checkpoint as it executes, and all of them —
transactions, effects, events, and each transaction's object diff — are staged in
`PendingCheckpointBuffer` until the seal writes them out in one atomic batch: object version
rows, tombstones, checkpoint-pinned version rows, the checkpoint summary and contents, and
every transaction's data, effects, and events. Nothing about a checkpoint is durable before
that commit, so a crash at any point loses only memory and a restart resumes from the
previous tip. The executor still gets read-your-writes for the next transaction's inputs,
but from the buffer rather than the store: the staged diffs double as a read overlay that
current, exact-version, and bounded object reads consult before the rpc-store. The overlay
wins whenever it has an entry, which is sound because a staged version always outranks
every persisted version of the same object — local execution Lamport-bumps past the live
version, and remote fetches are pinned at or below the fork checkpoint. The indexer ingests
each sealed checkpoint by reading it back out of the committed rows.

The derived indexes (owner, type, package-version, balance, bitmaps) are written for
local checkpoints by the embedded indexer alone, which runs every stock pipeline starting
one checkpoint after the fork point; the fork gets the full derived-index surface without
maintaining any of it. Sealing and publication are serialized through `Context`'s
publication lock, and publication blocks on the minimum watermark across every pipeline the
stock layer enables, so by the time an execution returns to its caller the checkpoint is
fully indexed, and any RPC read issued afterwards sees complete derived state. Subscribers
receive checkpoints from the indexer's broadcast pipeline, so their ordering is inherited
from indexing rather than from sealing.

Pre-fork state is the one exception, because it never flows through the indexer at all: the
indexer starts one checkpoint above the fork point and so never sees the state the fork
inherited. The seed load therefore writes those derived rows itself, but it does so through
the same `Restore` implementations the indexer's pipelines carry — `Objects`,
`ObjectByOwner`, `ObjectByType`, `Balance`, `PackageVersions`, and
`ObjectVersionByCheckpoint` anchored at the fork checkpoint — rather than through a parallel
set of writes maintained here. That matters beyond tidiness: a hand-written derived index has
to be kept in step with the schema it imitates, and the one place the fork previously did
that, an inverse of `Balance::restore` used to retract a coin's contribution, was exactly the
kind of duplicate that goes stale silently.

The `SimulatorStore` write surface cannot return errors, so a failed persist panics rather
than letting execution continue on state that has diverged from disk. An indexer stoppage
is likewise surfaced the moment it happens, because the startup loop watches for it as a
liveness watchdog, instead of appearing later as a publication timeout.

## Seeding

Seeding is how the fork acquires pre-fork state that no read can ask for by key. A user names
addresses and objects at startup; the fork resolves them against the live network and
loads them, once, before anything executes.

It happens once because it cannot happen twice. The enumeration behind an address seed is a
question about state at a checkpoint — which objects this address held at the fork point —
and it is answerable only while the remote still retains ownership data for that checkpoint,
which is a window of roughly an hour on the hosted endpoints. A fork that deferred the
question until something asked would find it unanswerable by then. So the fork asks up front,
and what it gets back is the boundary of everything it can later say about ownership.

Resolution and loading are separate steps for that reason. Resolution enumerates and records
object *references* in `seed_manifest.json`, which is written exclusively and never rewritten;
loading fetches what those references contain and hands each object to the `Restore`
pipelines. Only the first step is time-critical. The second is a set of exact `(id, version)`
keys, and an exact version is immutable, so it can be fetched whenever — which is also why an
address that could not be enumerated must not be recorded in the manifest at all. A partial
answer written down as a complete one is worse than no answer, because nothing afterwards
distinguishes the two.

The load runs as a single atomic batch carrying its own completion marker. Both halves of
that are load-bearing. Atomicity is what lets the load write blind — no reconciliation
against existing state, no retraction of stale rows — because it precedes execution, so
either the whole seed set is present or none of it is, and there is never a half-loaded fork
to reason about. The marker is what makes it unrepeatable: `Balance` accumulates through a
merge operator, so a replayed load would silently double every seeded coin. The marker lives
in the database rather than beside it, in the framework's restore column family under a
synthetic key, precisely so it commits with the rows it describes; a JSON sidecar could not
make that claim. A resumed fork finds the marker and skips.

Nothing maintains the seeded indexes afterwards, and nothing needs to. When a seeded object
moves, the checkpoint that moved it is a local checkpoint, and the indexer reads each such
checkpoint as a diff: inputs retract at the prior key, outputs write at the posterior one. The
seeded row is retracted by the same mechanism that maintains every other row.

## Where reads resolve

A fork is a chain that diverged from another at a checkpoint. It shares history at or below
the fork point, which belongs to both chains and which the fork can obtain on demand 
(with some limitations) from the live network, and the fork's own history above that forked checkpoint
which exists only locally.

The real network is consulted only at startup for initializing various indexes, and for shared history prior to the
forked checkpoint. The latter is done through queries pinned at the fork point.

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
read happened to need, so the highest stored row need not be the live one, and finding
nothing stored cannot distinguish an object that was removed from one that was never
fetched. That distinction decides whether to consult the live network RPC at all, and it is
`object_version_by_checkpoint` that carries it: a tombstone row means removed and is
authoritative, no row means never fetched and sends the read to the remote. "The object live
state" below sets out what may write to that index and why.

- A request bounded by a version is the subtle one. It asks for the highest version at or
below some bound, which is how child objects are read during execution. A stored row at or
below the bound is only trustworthy if the fork knows nothing newer can exist below it, which
holds when that row carries live-state authority or is a tombstone. Absent that, the sparse
cache may hold some older version that an unrelated historical read left behind, and serving
it would be wrong, so the bound must also be resolved against the live network and the
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
resolves three type-keyed lookups — `CoinMetadata`, `TreasuryCap`, `RegulatedCoinMetadata`
for the coin type — and composes them into one answer, so it belongs to this section rather
than to the enumerations below: an enumeration is complete over an owner, and nothing
enumerates the coin types a fork might be asked about, so there is no scan to take in
advance. Each of the three should resolve on demand, as a targeted type-keyed lookup that
caches like any other object read and claims completeness over nothing.

That is not what happens today. The three lookups read the local type index, which nothing
populates on demand, so the answer is bounded by what the seed and local execution happened
to write. Coin types published inside the fork resolve; pre-fork ones largely do not, because
`CoinMetadata` and `RegulatedCoinMetadata` are frozen after creation and the seed admits only
address-owned objects. See "Known gaps".

Events are the same principle seen from the other side. They need no policy of their own
precisely because the transaction read that produces them has one: whatever pulled the
transaction in pulled its events with it. That holds only while every path to the events
reaches them through a transaction, which is a property of the callers rather than of this
crate, and one worth stating because nothing here enforces it.

### Requests keyed by nothing: indexes and enumeration

Reads that enumerate rather than look up split into two kinds.

A *state* index answers what is true as of a checkpoint: which objects an address owns,
which objects have a type, what an address's balance is, which versions of a package exist.
Because that is a question about state at a point in time, the live network can answer
it pinned at the fork point — but only for as long as the remote retains ownership data for
that checkpoint, and only if something thought to ask.

Nothing asks at read time. These indexes are populated once, by the seed, and read locally
ever after; an owner, parent, or type outside the seed set answers empty. That is a real
limit and it is worth being plain about which way it fails: an empty answer is
indistinguishable from a complete one that found nothing, so a client cannot tell "this
address owns nothing" from "this fork was never told about this address."

The alternative was to run the enumeration on first read, and it was tried. What sinks it is
not cost but timing and honesty. A read arriving an hour after the fork point finds the
ownership window closed, so the scan that was supposed to make the answer complete cannot
run — and the read still has to return something. Worse, the scans that would be most useful
to defer are precisely the ones with no enumerable domain: nothing says in advance which
dynamic-field parents or which coin types a fork will be asked about, so there is no scan to
take up front and no bound on how many might be needed later. Seeding draws the boundary
where the user can see it, at startup, instead of leaving it to be discovered one failed read
at a time.

Local execution maintains the answer forward from there, so later reads are purely local.
Completeness is the point, so a partial answer is worse than none: a scan that could not run
must never be recorded as one that ran and found nothing.

A *log-position* index answers what sits at a given position in the total order of every
transaction ever executed. That is not a function of state at any checkpoint, and no query
pinned anywhere can produce it. The fork inherits its position numbering from the fork point
and starts writing at the next position, so every position below is a real transaction on
the other chain that this fork does not hold and cannot obtain. The fork's ledger begins at
the fork point, and this is a permanent property of forking rather than a missing feature.


## Data-dir layout

```
{root}/
  fork_metadata.json        network + fork checkpoint + chain id (validated on open)
  seed_manifest.json        immutable seed record (exclusive create)
  rpc_store/                stock sui-rpc-store RocksDB (RpcStoreSchema)
```

## Storing data in the same DB and CFs and tradeoffs considered

The design presented in this document stores all historical data in the same RocksDB instance,
and uses the same column families used for storing locally executed raw data and indexes.
This has the advantage of simplicity, but the fork requires to have a policy to ensure
that the correct data is served or fetched from the remote RPC as needed.

An alternative that we discussed was to use the same RocksDB instance, but to 
store the historical data in a separate set of column families. This would have the advantage
that we can simply embed the indexer from `sui-rpc-store` and not worry about possible
future changes to the schemas in the `sui-rpc-store` crate. It also allows to create custom
schemas if more data is required compared to the stock `sui-rpc-store` columns. 
Finally, it decouples the fork's local history data from the locally executed data and indexes.

Ultimately, as currently there does not seem to be a need to add additional data or column families
to the current schemas in `sui-rpc-store`, it seems reasonable to use the same column families for both historical and locally executed data.

## Known gaps

Fork directories written before the seal became a single batch may hold orphaned object
rows from a crash mid-publication — rows a checkpoint wrote whose seal never landed.
Nothing sweeps those on open; the atomic seal only guarantees that no new ones appear.

Type layout resolution reaches under the object policy rather than composing over it. It
loads the packages a type references straight from stored rows, so a pre-fork type whose
package has never been materialized resolves to no layout at all, even though an object
read for that same package would have fetched it. Every read that renders Move values as
JSON depends on this. The same shortcut also picks packages by scanning stored versions,
which is safe for ordinary packages, immutable at one version per id, but not for system
packages, which carry every version they have ever had under one id.

Coin metadata does not resolve for pre-fork coin types. `get_coin_info` composes three
type-keyed lookups, but it takes them from the local type index rather than resolving each on
demand. That index has two writers — the seed load and the indexer — so the method answers
for any coin type published inside the fork, and for a pre-fork `TreasuryCap` whose holder was
seeded as an address, since that is an ordinary address-owned object. What it cannot answer
for is the frozen pair: `CoinMetadata` and `RegulatedCoinMetadata` are immutable after
creation, seed resolution admits only address-owned objects, and no read-time path fetches
them. The gap is therefore a coin type's *history*, not the method. Closing it means what
"Derived reads compose over policy" already prescribes: resolve each wrapper as a targeted
type-keyed lookup against the live network when something asks, and cache the resulting
object like any other. This is not a limit of forking — unlike the ledger range below — just
work not done.

The remote leg of a bounded read is not pinned at the fork point. Where the latest and
exact-version object queries carry the fork checkpoint explicitly, the bounded query carries
only its version bound, so it resolves against the live network's *current* state.
Post-fork history on the other chain can therefore answer a read inside this fork, which is
the one thing every other remote path is built to prevent. It reaches `read_child_object` on
both the RPC and executor paths, so it can skew execution rather than only reads.

That gap and the one below compound rather than merely coexisting, which is worth noticing
before either is fixed: the fix proposed below is to merge the remote `RootVersion(bound)`
result with the local candidate, and merging in an unpinned answer would make the remote leg
authoritative precisely where it is least trustworthy. Pinning has to land first.

Bounded child reads can also serve stale history. `get_object_lt_or_eq_version` trusts the
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
