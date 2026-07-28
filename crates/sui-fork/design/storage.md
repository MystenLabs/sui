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

A fork is a chain that diverged from another at a checkpoint. Call that checkpoint the fork
point. Its world is made of two disjoint parts: the shared history at or below the fork
point, which belongs to both chains and which the fork can obtain on demand from the
forked-from chain, and the fork's own history above it, which exists nowhere else. Every
read is a question about that composite world, and three rules decide how it is answered.

Local knowledge, when it is authoritative, always wins. The forked-from chain is consulted
only for shared history, and only through queries pinned at or below the fork point.
Anything the forked-from chain finalized *after* the fork point is never admitted, because
that history did not happen here — the two chains disagree from the fork point onward, and
importing the other one's later state would silently merge two worlds.

What differs between reads is only how the request lets the fork decide which part of the
world it is asking about. That decision is made by the kind of key the request carries.

### Requests keyed by a checkpoint

The key already answers the question. A checkpoint above the fork point can only have been
produced by this fork, so it is served locally and a miss is a final answer; asking the
forked-from chain would return a checkpoint that happens to share a sequence number while
containing different transactions. A checkpoint at or below the fork point is shared
history, so a local miss is resolved from the forked-from chain, pinned at that checkpoint,
and persisted so the question is not asked twice.

Reads for the *latest* checkpoint ask a different question, and they always mean the
fork's own tip. A fork that has executed nothing is at
the fork point; a fork that has executed is ahead of it. The forked-from chain's tip is
irrelevant and must never be consulted.

### Requests keyed by a digest

A digest carries no information about which side of the fork point it falls on, so the fork
cannot classify the request before answering it. Transactions, their effects, and the
checkpoint that finalized them all have this shape.

Local knowledge is tried first and is authoritative when present. On a miss the forked-from
chain is asked, and its answer includes the checkpoint that finalized the transaction. That
checkpoint is what classifies the request after the fact: at or below the fork point it is
shared history and is persisted and returned, and above it the transaction belongs to the
other chain's divergent future and the correct answer is that it does not exist here. A
digest unknown to both is simply unknown.

### Requests keyed by an object

An object can be asked about in three ways, and they are not variations of one rule.

An exact version is an immutable key: a given version of an object never changes, so a local
row can be served without further thought, and a miss can be resolved from the forked-from
chain pinned at the fork point. Pinning is what enforces divergence here — a version created
after the fork point does not exist in a query pinned at it, so no separate guard is needed.

A request with no version asks what is *current*, which is a question about the fork's own
state rather than about history, and it is the one object read that cannot be answered from
stored object rows alone. The fork holds versions sparsely, caching whatever some earlier
read happened to need, so the highest stored version is not necessarily the live one, and
finding nothing stored cannot distinguish an object that was removed from one that was never
fetched. That distinction decides whether to consult the forked-from chain at all, which is
why currency is tracked explicitly; the following section describes how.

A request bounded by a version — the highest version at or below some bound, which is how
child objects are read during execution — is the subtle one. A stored row at or below the
bound is only trustworthy if the fork knows nothing newer can exist below it, which holds
when that row carries currency authority or is a tombstone. Absent that, the sparse cache
may hold some older version that an unrelated historical read left behind, and serving it
would be wrong, so the bound must also be resolved against the forked-from chain and the
higher of the two answers taken.

### Derived reads compose over policy

Some reads are not lookups at all but functions of other reads. A transaction's events are
stored with the transaction. A type's layout is a function of the packages it references,
and packages are objects.

Such reads must resolve their inputs through the fork's own read policy rather than by
reaching into stored rows directly. A layout resolved by loading packages straight from
local storage cannot render a pre-fork type whose package has never been materialized, even
though the object policy one layer down would have fetched it happily. The rule is that
composing a derived read out of raw storage discards the policy that makes its inputs
correct, so derived reads compose over the policy, never under it.

Events are the same principle seen from the other side. They need no policy of their own
precisely because the transaction read that produces them has one: whatever pulled the
transaction in pulled its events with it. That holds only while every path to the events
reaches them through a transaction, which is a property of the callers rather than of this
crate, and one worth stating because nothing here enforces it.

### Requests keyed by nothing: indexes and enumeration

Reads that enumerate rather than look up split into two kinds, and conflating them is the
easiest mistake to make here.

A *state* index answers what is true as of a checkpoint — which objects an address owns,
which objects have a type, what an address's balance is, which versions of a package exist.
Because that is a question about state at a point in time, the forked-from chain can answer
it pinned at the fork point. The fork therefore takes one complete enumeration there,
records that it did so, and lets local execution maintain the answer forward; later reads
are purely local. Completeness is the point, so a partial answer is worse than none: a scan
that could not run must never be recorded as one that ran and found nothing.

A *log-position* index answers what sits at a given position in the total order of every
transaction ever executed. That is not a function of state at any checkpoint, and no query
pinned anywhere can produce it. The fork inherits its position numbering from the fork point
and starts writing at the next position, so every position below is a real transaction on
the other chain that this fork does not hold and cannot obtain. The fork's ledger begins at
the fork point, and this is a permanent property of forking rather than a missing feature.

### Absence must not be mistaken for emptiness

Three of the cases above have a range the fork cannot serve: positions below the fork point
in the ledger, enumerations over pre-fork history, and any pre-fork read at all when the
fork point falls outside the window of history the forked-from chain still retains. In each
the natural failure is an empty result, which a client cannot tell from a true answer of
"nothing matches."

A read that cannot be served must therefore be distinguishable from one that was served and
found nothing. This matters most where the fork records its own conclusions: an enumeration
that could not run must not be marked complete, because a fork that caches "this address
owns nothing" from a scan that never happened will keep answering that way forever.

## The current-version authority

Currency is recorded in `object_version_by_checkpoint`, which maps `(ObjectID, checkpoint)`
to the version the object ended that checkpoint at, and which the embedded indexer already
maintains for every checkpoint the fork executes. The owner and type indexes also record
latest live versions, but they are keyed by owner and type and cover only indexed objects,
so neither can answer for an arbitrary id. A currency read is a floor scan over the version
index, bounded at
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

Type layout resolution reaches under the object policy rather than composing over it. It
loads the packages a type references straight from stored rows, so a pre-fork type whose
package has never been materialized resolves to no layout at all, even though an object
read for that same package would have fetched it. Every read that renders Move values as
JSON depends on this. The same shortcut also picks packages by scanning stored versions,
which is safe for ordinary packages, immutable at one version per id, but not for system
packages, which carry every version they have ever had under one id.

An inventory scan that could not run is recorded as one that ran and found nothing. The
forked-from chain retains only a window of history, and a fork point below that window
cannot be enumerated at all; the fork already learns the window's floor and reports it to
clients, but does not check its own fork point against it. A scan attempted below the
window returns empty, is marked complete, and that emptiness then answers every later read
for that owner.

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
