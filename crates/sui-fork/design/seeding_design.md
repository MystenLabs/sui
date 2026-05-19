# Seeding Design

## Problem

The forking tool can execute transactions against forked state but cannot answer
"what objects does address X own?" unless the fork has durable ownership metadata.
For example, if a user would like to execute a transaction, the forked network does not have
yet the ownership information - which gas coins it owns. To enable this, seeding
records object reference metadata at startup so the owned-object index can be
initialized when it is first needed.

Seeding resolves object refs (id, version, digest) at startup, establishing
the minimal manifest needed to build a complete ownership index later.

Object contents are NOT fetched during seeding. Content fetching is deferred
until an actual object read or lazy owned-object index initialization needs the
full object BCS.

### Checkpoint age constraint

- **Recent checkpoints (<1h)**: GraphQL `address.objects()` works — can seed by address
- **Older checkpoints**: Only individual object lookups work — must seed by object ID

## CLI

Two new flags on the `start` command:

```
--address <ADDR>     Seed all owned objects for this address (repeatable)
--object <ID>        Seed a specific object ID if it is address-owned (repeatable)
```

Both can be combined. Addresses and explicit object IDs are resolved into seed
entries with object refs. Results are merged and deduped by object ID.
It is important to note if addresses is passed, the checkpoint must be within
the last 1h.

When restarting a fork with the same `--data-dir` and `--checkpoint`, the existing
seeding information that is stored on disk will be reused. If `--address` or `--object` is
provided, the tool will error with a message indicating that a seed manifest already exists.
The owned-object index and local object `latest` state remain authoritative over the
manifest after the fork has initialized the index or executed local transactions.

## Generated files

One file is written to `{data_dir}/{network}/forked_at_{checkpoint}/`:

### `seed_manifest.json` — internal refs for index rebuilding

```json
{
    "network": "testnet",
    "checkpoint": 12345678,
    "entries": [
        {
            "object_ref": ["0x...", 42, "..."]
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

### Address objects query

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
- Collect every returned node as an object ref. `Address.objects` is the source
  of truth for address-owned membership and the manifest does not persist owner.

### Individual objects query

For explicit `--object` IDs, use a metadata-only `multiGetObjects` query. The
provided object ID is used as the object ref ID; the query does not request
`Object.address` or `objectBcs`.

```graphql
query($keys: [ObjectKey!]!) {
  multiGetObjects(keys: $keys) {
    version
    digest
    owner {
      ... on AddressOwner { address { address } }
      ... on ConsensusAddressOwner { address { address } }
    }
  }
}
```

The query returns object refs for seed entries after confirming address
ownership. Object type, balance, owner, and full BCS are not part of the seed
manifest.

## Implementation Details

**Types**

```rust
#[derive(Serialize, Deserialize)]
struct SeedEntry {
    object_ref: ObjectRef,
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
3. For remaining object IDs → metadata-only object seed query, collect entries
4. Merge, dedup by `object_id`
5. Return `SeedManifest`

## Composition with owned_objects_design.md

The seed manifest is the immutable pre-fork baseline. The
`indices/owned_objects` file and object `latest` metadata are the current local
state once the fork has started executing.

**Index initialization**:
1. If `indices/owned_objects` exists, use it as-is and do not rewrite it from the manifest.
2. If the index is missing and the fork has not advanced past the fork checkpoint, initialize it
   from `seed_manifest.json` before the first owned-object read or local execution update.
3. If the index is missing but local checkpoints are newer than the fork checkpoint, fail closed
   instead of rebuilding stale seed state over local mutations.

During initialization, each seed entry is fetched at the exact seeded version and fork checkpoint,
validated against the manifest ref, written to the local object cache, and converted into a
complete owned-object index entry with owner, object type, and optional coin balance.

**When a seeded object is deleted/mutated post-fork**:
- `update_objects()` removes the old index entry, adds the new address-owned entry, or removes the
  entry for wrapped/shared/immutable/object-owned transitions
- local object `latest` removal state prevents current-object reads from falling back to the remote endpoint

**Key invariant**: Seed manifest is immutable after creation. Post-fork state is tracked entirely
through the owned-object index, object BCS files, and object `latest` state.

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
| `src/seed.rs` | New — types, seed policy, manifest resolution |
| `src/gql/queries.rs` | Modify — new `address_owned_objects_query` module |
| `src/cli.rs` | Modify — seed CLI args on Start |
| `src/startup.rs` | Modify — wire seeding into initialize() |
| `src/filesystem.rs` | Modify — seed manifest read/write |
| `src/store.rs` | Modify — lazy owned-object index initialization |
| `src/lib.rs` | Modify — add `mod seed` |

## Implementation order

1. Types in `src/seed.rs` + `mod seed` in `lib.rs`
2. GraphQL query (`address_objects_query`)
3. Filesystem manifest read/write
4. Resolution logic (`resolve_seeds()`)
5. CLI args + plumbing through `cmd_start` → `initialize`
6. Lazy index initialization in `DataStore`
7. Tests
