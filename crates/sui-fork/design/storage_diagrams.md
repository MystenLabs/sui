<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# `sui-fork` storage: read and write flow diagrams

Companion to [storage.md](storage.md), which argues the design; this file draws it. Every
read diagram below is a variation on one rule: answer locally when local knowledge is
authoritative, fork to the remote chain — GraphQL pinned at the fork checkpoint — when it
is not, and persist what came back so the same question is never asked twice. Writes never
fork: local execution is the only writer of post-fork state, and remote data enters the
store only as the persisted result of a read.

## Who holds whom

`ServiceManager` opens and owns the durable pieces; `ForkStore` orchestrates policy over
them; `RpcReader` and Simulacrum are the two consumers standing in front of it.

```mermaid
flowchart TD
    rpcsvc["RpcService (sui-rpc-api)"] --> reader[RpcReader]
    sim["Simulacrum (executor)"] --> fs[ForkStore]
    reader -->|fork-policy reads| fs
    reader -->|stock-only reads| stock["RpcStoreReader (stock)"]
    fs --> ls[LocalStore]
    fs --> rs[RemoteSource]
    fs --> inv[InventoryInitializer]
    fs --> pending[PendingCheckpointBuffer]
    fs --> meta
    inv --> rs
    inv --> ls
    inv --> meta["MetadataStore (sidecar files)"]
    ls --> stock
    ls --> db[("rpc_store RocksDB")]
    ls --> lsdb[("live_state RocksDB")]
    stock --> db
    rs --> gql["GraphQL, pinned at the fork checkpoint"]
    svc[ServiceManager] -->|opens and owns| db
    svc -->|opens and owns| lsdb
    svc -->|runs| idx["embedded rpc-store Indexer"]
    idx --> db
    sim -.->|sealed checkpoints are read back for ingestion| idx
```

## Reads

### RPC routing

`RpcReader` is a thin router: everything the fork has policy for goes to `ForkStore`
(which is itself local-first), and only the surfaces the fork has no policy for touch the
stock reader directly.

```mermaid
flowchart TD
    req[incoming gRPC read] --> route{RpcReader}
    route -->|"fork policy: objects, transactions,<br/>checkpoints, owner and coin indexes"| fs[ForkStore]
    route -->|"stock-only: events, full contents, committees,<br/>epoch info, layouts, ledger and bitmap indexes"| stock[RpcStoreReader]
    route -->|"hybrid: chain identifier,<br/>highest indexed checkpoint"| hybrid[stock first, fork fallback]
    hybrid --> stock
    hybrid --> fs
    stock --> db[(rpc_store)]
    fs --> resolve["local-first resolution<br/>(diagrams below)"]
```

### Latest-object reads: the three-way fork

A latest read cannot trust a reverse scan of the sparse `objects` family, so it consults
the `LiveState` pointer, whose three states map exactly onto the three outcomes. Note the
`Removed` arm: an authoritative tombstone must never be "resurrected" by a remote fetch.

```mermaid
flowchart TD
    go["get_object(id)"] --> ptr{"LiveState pointer for id"}
    ptr -->|"Live(v)"| row["read the objects row (id, v) locally"] --> ret[return object]
    ptr -->|Removed| gone["return not found<br/>(authoritative, never ask the remote)"]
    ptr -->|absent| remote["RemoteSource: object<br/>at the fork checkpoint"]
    remote -->|found| persist["persist the row,<br/>then set the pointer"] --> ret
    remote -->|never existed| miss[return not found]
```

### Immutably-keyed reads: exact versions, transactions, checkpoints

A row under an immutable key cannot go stale, so the local rpc-store row is served
directly; only a miss forks to the remote. The tombstone arm applies to object reads —
transactions and checkpoints have no removal states. `RemoteSource` guards the fallback:
a result finalized after the fork point must not leak into the diverged fork.

```mermaid
flowchart TD
    key["read by immutable key:<br/>object at version, transaction or<br/>checkpoint by digest or sequence"] --> local{"row in local rpc_store?"}
    local -->|live row| ret[return]
    local -->|"tombstone (objects only)"| gone[return not found]
    local -->|missing| remote{"RemoteSource query,<br/>pinned at the fork checkpoint"}
    remote -->|"finalized after the fork point"| dropped["dropped by the pre-fork guard:<br/>return not found"]
    remote -->|found pre-fork| persist["persist it<br/>(a transaction also persists its<br/>checkpoint and events)"] --> ret
    remote -->|not found| miss[return not found]
```

### Owner and index reads: lazy inventories

Owner-scoped reads cannot be answered by fetching single objects — completeness is the
point — so the first such read triggers a one-time full enumeration, recorded in a
completion marker so every later read is purely local.

```mermaid
flowchart TD
    idx["owned-objects, dynamic-field,<br/>coin or balance read"] --> marker{"inventory marker complete?<br/>(MetadataStore)"}
    marker -->|yes| serve[iterate the local index families]
    marker -->|no| scan["full remote enumeration<br/>at the fork checkpoint"]
    scan --> backfill["backfill index rows and derived rows,<br/>synchronously"]
    backfill --> mark[write the completion marker]
    mark --> serve
```

## Writes

### Local execution, sealing, and indexing

Everything canonical is written synchronously — the executor needs read-your-writes and
the indexer re-reads sealed rows — while everything derived is left to the embedded
indexer. Two orderings keep crashes fail-safe rather than fail-open: rows commit before
the `LiveState` pointer that makes them authoritative, and removals stage before writes
within one diff so a wrapped-then-recreated object lands live. A failed persist panics:
the `SimulatorStore` surface cannot return errors, and executing past one would diverge
memory from disk.

```mermaid
flowchart TD
    tx[Simulacrum executes a transaction] --> stage["stage tx, effects, events in<br/>PendingCheckpointBuffer (memory)"]
    tx --> diff[apply the object diff under the snapshot lock]
    diff --> removals[stage removals before writes]
    removals --> rows[commit object rows and tombstones to rpc_store]
    rows --> ptrs["update LiveState pointers,<br/>after the rows commit"]
    ptrs --> seal["create_checkpoint seals:<br/>summary, contents, every staged tx row"]
    seal --> clear[drop the staged entries]
    clear --> ingest[embedded Indexer ingests the sealed checkpoint]
    ingest --> derived["write derived indexes:<br/>owner, type, balance, package, bitmaps"]
    derived --> wm[publication blocks on the minimum pipeline watermark]
    wm --> pub[broadcast the checkpoint to subscribers]
```

Pre-fork state is the one exception to "derived rows come from the indexer": seed saves,
inventory scans, and lazy materialization write their derived rows synchronously alongside
the objects they persist. That creates no second writer — those saves cover versions at or
before the fork checkpoint, a range the indexer, which starts one checkpoint after the
fork point, never touches.
