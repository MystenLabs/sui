# Seeding Design

## Problem

The forking tool can execute transactions against forked state but cannot answer
"what objects does address X own?" without explicit rpc querying at startup / before first transaction.
For example, if a user would like to execute a transaction, the forked network does not have
yet the ownership information - which gas coins it owns. To enable this, seeding
allows the tool to resolve ownership information at startup and build an initial ownership index.

Seeding resolves object refs (id, version, digest) + lightweight metadata (owner, type)
at startup, establishing the data needed to build an ownership index.

Object contents are NOT fetched during seeding for address-based seeds. Content
fetching is deferred to the existing `get_object_from_remote()` path used by
execution. For explicit `--object` IDs, we reuse the existing `multiGetObjects`
infrastructure which does fetch BCS — a useful side-effect since these objects
will likely be accessed during execution anyway.

### Checkpoint age constraint

- **Recent checkpoints (<1h)**: GraphQL `address.objects()` works — can seed by address
- **Older checkpoints**: Only individual object lookups work — must seed by object ID

## CLI

Two new flags on the `start` command:

```
--address <ADDR>     Seed all owned objects for this address (repeatable)
--object <ID>        Seed a specific object by ID (repeatable)
```

Both can be combined. Addresses are resolved into object refs; explicit
object IDs are fetched via the existing `multiGetObjects` path. Results are
merged and deduped by object ID. It is important to note if addresses is passed,
the checkpoint must be within the last 1h.

When restarting a fork with the same `--data-dir` and `--checkpoint`, the existing
seeding information that is stored on disk will be reused. If `--address` or `--object` is
provided, the tool will error with a message indicating that a seed manifest already exists.
The durable owned-object index and local deleted markers remain authoritative over the manifest
after the fork has executed local transactions.

## Generated files

One file is written to `{data_dir}/{network}/forked_at_{checkpoint}/`:

### `seed_manifest.json` — internal, full metadata for index rebuilding

```json
{
    "network": "testnet",
    "checkpoint": 12345678,
    "entries": [
        {
            "object_id": "0x...",
            "version": 42,
            "digest": "...",
            "owner": "0x...",
            "object_type": "0x2::coin::Coin<0x2::sui::SUI>",
            "balance": 1000000
        }
    ]
}
```

## Seed resolution workflow

The seed manifest is immutable once written.

Seed resolution:
   - seed_input provided + manifest exists on disk → ERROR:
     "A seed manifest already exists at <path>. To fork the same
      checkpoint with different seeds, use a different --data-dir."
   - seed_input provided, no existing manifest → resolve → write manifest + generated file
   - no seed_input, manifest exists on disk → read it (restart case)
   - no seed_input, no manifest → proceed without seeds

## GraphQL queries

### Address-owned objects query

`src/gql/queries.rs` owns the checkpoint-scoped address query. The seed module
calls `GraphQLClient::get_address_owned_objects_at_checkpoint()` and does not
construct raw GraphQL requests.

Uses `Checkpoint.query` scoping (same pattern as `VersionAtCheckpointQuery`):

```graphql
query($sequenceNumber: UInt53, $address: SuiAddress!, $first: Int, $after: String) {
  checkpoint(sequenceNumber: $sequenceNumber) {
    query {
      address(address: $address) {
        objects(first: $first, after: $after) {
          nodes {
            address
            version
            digest
            owner {
              ... on AddressOwner { address { address } }
            }
            contents {
              type { repr }
              json
            }
          }
          pageInfo { hasNextPage endCursor }
        }
      }
    }
  }
}
```

- Page size: 50
- Paginate with cursor loop (same pattern as `events_query`)
- Only collect `AddressOwner` entries. Other owner variants are handled by the
  query fallback and skipped; they are not usable as address-owned gas/input
  objects for the initial index.

### Individual objects (reuse existing)

For explicit `--object` IDs, reuse the existing `object_query::query()`
infrastructure (`multiGetObjects` with `ObjectFragment` that fetches `objectBcs`).
BCS gets cached to disk. After fetching, extract metadata (owner, type, version,
digest) from the deserialized `Object` for the `SeedEntry`.

## Implementation Details

**Types**

```rust
#[derive(Serialize, Deserialize)]
struct SeedEntry {
    object_id: ObjectID,
    version: SequenceNumber,
    digest: ObjectDigest,
    owner: SuiAddress,
    object_type: StructTag,
    balance: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct SeedManifest {
    network: String,
    checkpoint: u64,
    entries: Vec<SeedEntry>,
}

/// Aggregated input from CLI args before resolution
struct SeedInput {
    addresses: Vec<SuiAddress>,
    object_ids: Vec<ObjectID>,
}
```

**Resolution logic**

```rust
async fn resolve_seeds(
    input: &SeedInput,
    checkpoint: CheckpointSequenceNumber,
    gql: &GraphQLClient,
) -> Result<SeedManifest>
```

1. For each address → paginate `address_objects_query`, collect entries
2. Collect explicit object IDs not already resolved by address seeding (dedup)
3. For remaining object IDs → `object_query::query()` (existing), extract metadata
4. Merge, dedup by `object_id`
5. Return `SeedManifest`

## Composition with owned_objects_design.md

The seed manifest is the immutable pre-fork baseline. The durable
`indices/owned_objects` file and object tombstones are the current local state
once the fork has started executing.

**Index rebuild on startup** (two sources):
1. If `indices/owned_objects` exists, use it as-is and do not rewrite it from the manifest.
2. If the index is missing and the fork has not advanced past the fork checkpoint, initialize it
   from `seed_manifest.json`.
3. If the index is missing but local checkpoints are newer than the fork checkpoint, fail closed
   instead of rebuilding stale seed state over local mutations.

**When a seeded object is first accessed** (e.g., as tx input):
- `get_object_from_remote()` fetches full BCS, writes to disk
- the owned index entry can already satisfy default owned-object listing, while object-loading
  read masks fetch BCS lazily

**When a seeded object is deleted/mutated post-fork**:
- `update_objects()` removes the old index entry, adds the new address-owned entry, or removes the
  entry for wrapped/shared/immutable/object-owned transitions
- local deleted markers prevent current-object reads from falling back to the remote endpoint

**Key invariant**: Seed manifest is immutable after creation. Post-fork state is tracked entirely
through the durable owned-object index, object BCS files, and deleted markers.

## Edge cases

- **Address resolves to 0 objects**: warn, continue
- **Object ID not found at checkpoint**: warn, skip (don't fail startup)
- **Duplicate object IDs across sources**: dedup silently
- **Checkpoint too old for address seeding**: surface clear error with guidance
  to use `--object` instead
- **Network error during resolution**: fail startup, no partial manifest written

## Files to modify/create

| File | Action |
|------|--------|
| `src/seed.rs` | New — types, seed policy, manifest/index initialization |
| `src/gql/queries.rs` | Modify — new `address_owned_objects_query` module |
| `src/cli.rs` | Modify — three new CLI args on Start |
| `src/startup.rs` | Modify — wire seeding into initialize() |
| `src/filesystem.rs` | Modify — seed manifest read/write |
| `src/store.rs` | Modify — expose `gql()` and `local()` accessors |
| `src/lib.rs` | Modify — add `mod seed` |

## Implementation order

1. Types in `src/seed.rs` + `mod seed` in `lib.rs`
2. GraphQL query (`address_objects_query`)
3. Filesystem manifest read/write
4. Resolution logic (`resolve_seeds()`)
5. CLI args + plumbing through `cmd_start` → `initialize`
6. Store accessors
7. Tests
