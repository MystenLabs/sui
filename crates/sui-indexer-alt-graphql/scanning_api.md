---

## TLDR

Queries with multiple filters on transactions or events might be slow. Some combination of filters return 0 rows, resulting in inefficient queries where full tables have to be scanned to find no matches. Some filters may result in a large gap between two matching rows which result in a large number of rows scanned to filter out rows.

**Implemented Solution:**

- **Two bloom filter approach** for efficient checkpoint filtering:
  - `cp_bloom_blocks`: filter covering 1000 checkpoints per block (256KB total)
  - `cp_blooms`: filter per checkpoint (16KB before folding, 1kb min after folding)
- **GraphQL APIs** for scanning with multiple AND filters:
  - `scanTransactions`: Query transactions by function, affectedAddress, affectedObject, sentAddress
  - `eventsScan`: Query events by sender, module, or type
- Both APIs use bloom filters to quickly eliminate checkpoints that can't contain matches, then load and exact-filter candidates

---

## Database Tables

### `cp_bloom_blocks` (checkpoint range filter)

Stores bloom filters aggregated over blocks of 1000 checkpoints. Used for initial coarse filtering to quickly eliminate large ranges.

```sql
CREATE TABLE cp_bloom_blocks (
    -- Checkpoint block ID (cp_sequence_number / 1000)
    cp_block_index BIGINT,
    -- Index of this bloom block within the 128-block filter (0-127)
    bloom_block_index SMALLINT,
    -- Bloom filter bytes for this block (2KB each)
    bloom_filter BYTEA NOT NULL,
    PRIMARY KEY (cp_block_index, bloom_block_index)
);
```

**Parameters:**

- Block size: 1000 checkpoints per block
- Bloom blocks: 128 blocks × 2KB = 256KB total per checkpoint block
- Hash functions: 5
- Seed: `cp_block_index` (varies per block for better distribution)

**Estimated size (250M checkpoints):**

- 250,000 checkpoint blocks × 128 rows × 2KB = **64GB**

### `cp_blooms` (per-checkpoint filter)

Stores bloom filters for individual checkpoints. Used for per-checkpoint filtering after block-level matches. Sparse blooms filters are folded down to 40% density or 1kb.

```sql
CREATE TABLE cp_blooms (
    -- Checkpoint sequence number
    cp_sequence_number BIGINT PRIMARY KEY,
    -- Folded bloom filter bytes (1-16KB depending on density)
    bloom_filter BYTEA NOT NULL
);
```

**Parameters:**

- Initial size: 16KB (131,072 bits) before folding
- Hash functions: 6
- Seed: 67 (constant)
- Folding: minimum 1KB, stops when density exceeds 40%

**Estimated size (250M checkpoints):**

Sample distribution of bloom filters in 2.5M checkpoints
┌─────────────────┬───────────┬───────────┬────────────┐
│ Bits │ Count │ Avg Items │ Percentage │
├─────────────────┼───────────┼───────────┼────────────┤
│ 8,192 (1 KB) │ 2,415,982 │ 195.9 │ 99.96% │
├─────────────────┼───────────┼───────────┼────────────┤
│ 16,384 (2 KB) │ 1,072 │ 1,618.0 │ 0.04% │
├─────────────────┼───────────┼───────────┼────────────┤
│ 32,768 (4 KB) │ 4 │ 3,580.8 │ ~0% │
├─────────────────┼───────────┼───────────┼────────────┤
│ 65,536 (8 KB) │ 4 │ 8,374.8 │ ~0% │
├─────────────────┼───────────┼───────────┼────────────┤
│ 131,072 (16 KB) │ 7 │ 33,416.7 │ ~0% │
└─────────────────┴───────────┴───────────┴────────────┘

- Most checkpoints fold to minimum 1KB (typical checkpoint has few unique filter values)
- 250M × 1KB ≈ **~250GB (assuming every checkpoint has relevant txns)**

**Why Folding?**
**reduced I/O during queries**: each checkpoint membership check requires reading the entire bloom filter from the database. We allocate a large bloom filter to handle dense checkpoints (lots of relevant txns) and fold to reduce io for sparse checkpoints. Trading increased FPR from high frequency values always setting the same bits for better IO and storage.

---

## Indexer Pipelines

Two pipelines populate these tables:

**`cp_blooms` pipeline**: Processes each checkpoint, collects filter values and builds a bloom filter. Uses folding to balance storage size vs. false positive rate.

**`cp_bloom_blocks` pipeline**: Aggregates bloom filters from individual checkpoints into coarse blocks covering 1000 checkpoints each. We upsert on conflict because bloom filters are idempotent.

**Filter values collected:**

- Transaction senders (excluding system addresses 0x0, 0x3)
- Address owners from changed objects
- Object IDs from object changes (excluding clock 0x6)
- Package IDs from Move calls
- Event package addresses and type addresses

---

## Hashing Implementation

### Double Hashing

Bloom filters need k independent hash functions to set k bit positions per element. Rather than computing k separate cryptographic hashes, we use **double hashing** which generates k positions from just two hash values without any loss to asymptotic false positive rate ([Kirsch-Mitzenmacher 2006](https://www.eecs.harvard.edu/~michaelm/postscripts/esa2006a.pdf)).

**Algorithm:**

1. Compute one 64-bit hash using SipHash-1-3 with the seed
2. Split into h1 (full hash) and h2 (derived from upper 32 bits × constant)
3. Generate positions: `h1 = (h1 + h2).rotate_left(5)` for each iteration

```rust
pub struct DoubleHasher {
    h1: u64,
    h2: u64,
}

impl DoubleHasher {
    pub fn with_value(value: &[u8], seed: u128) -> Self {
        let mut hasher = SipHasher13::new_with_keys(seed as u64, (seed >> 64) as u64);
        hasher.write(value);
        Self::new(hasher.finish())
    }

    pub fn next_hash(&mut self) -> u64 {
        self.h1 = self.h1.wrapping_add(self.h2).rotate_left(5);
        self.h1
    }
}
```

**Notes:**

- **h2 multiplier** = `0x517cc1b72722_0a95` (≈ 2^64/π) - large number with mixed bits for good distribution
- **Rotation by 5** - coprime to 64, mixes high and low bits to avoid evenly spaced patterns
- **SipHash-1-3** - one call per value

### Blocked Bloom Filter Hashing

For `cp_bloom_blocks`, each value maps to exactly one of 128 blocks:

```rust
// Block selection: first hash determines block index
let block_idx = DoubleHasher::with_value(value, seed).next_hash() % 128;

// Bit positions: subsequent hashes with seed+1 set bits within that block
for h in DoubleHasher::with_value(value, seed + 1).take(NUM_HASHES) {
    set_bit(block, h % bits_per_block);
}
```

**Why separate seeds?** Using `seed` for block selection and `seed + 1` for bit positions prevents correlated patterns where block assignment and bit positions are derived from the same hash sequence.

### Why Blocked Bloom Filters?

A naive approach would store a single large bloom filter (256KB) per checkpoint block. The problem: to check membership, you'd need to load all 256KB even if you're only checking a few values.

**Blocked bloom filters** solve this by partitioning the filter into 128 separate 2KB blocks, where each value deterministically maps to exactly one block based on its hash. This enables:

1. **Sparse row access**: For a query with 3 filter values that hash to blocks 7, 42, and 99, we only load those 3 rows (6KB) instead of all 128 rows (256KB)

2. **Index-friendly queries**: Each block is a separate database row with `(cp_block_index, bloom_block_index)` as the primary key, enabling efficient index lookups

3. **Reduced I/O for selective queries**: Rare filter values (e.g., a specific package ID) typically map to just 1 block per checkpoint block, making queries very efficient

---

## GraphQL API

### Query Flow

### 1. Validate Bounds

Check that the scan range (`cp_hi - cp_lo`) doesn't exceed `maxScanLimit`. Return error if exceeded.

### 2. Build Bloom Probes

Convert filter values to byte arrays for bloom filter probing:

- Transaction filters: package ID, affected address/object, sender address
- Event filters: sender address, module package, type package

Probes are pre-computed as `(byte_offset[], bit_mask[])` arrays:

```rust
pub fn probe(values: impl IntoIterator<Item = impl AsRef<[u8]>>) -> BloomProbe {
    let mut byte_offsets = Vec::new();
    let mut bit_masks = Vec::new();
    for value in values {
        for bit_idx in hash(value) {
            byte_offsets.push(bit_idx / 8);
            bit_masks.push(1 << (bit_idx % 8));
        }
    }
    (byte_offsets, bit_masks)
}
```

The SQL `bloom_contains(filter, byte_offsets, bit_masks)` function checks that for each `(offset, mask)` pair: `(filter[offset] & mask) = mask`.

### 3. Coarse Filter (Block Level)

Query `cp_bloom_blocks` to find checkpoint blocks that might contain matches. Uses double
NOT EXISTS for universal quantification: "keep cp_block_index where no probe lacks a matching
bloom block." This avoids LEFT JOIN (NULL + STRICT function issues) and GROUP BY/HAVING COUNT
(no short-circuit), and lets PostgreSQL's anti-join short-circuit as soon as any probe fails.

```sql
WITH condition_data(cp_block_index, bloom_idx, byte_pos, bit_masks) AS (VALUES ...)

, blocked_matches AS (
    SELECT cd.cp_block_index, ...
    FROM (SELECT DISTINCT cp_block_index FROM condition_data) cd
    WHERE NOT EXISTS (
        SELECT 1 FROM condition_data c
        WHERE c.cp_block_index = cd.cp_block_index
          AND NOT EXISTS (
              SELECT 1 FROM cp_bloom_blocks bb
              WHERE bb.cp_block_index = c.cp_block_index
                AND bb.bloom_block_index = c.bloom_idx
                AND bloom_contains(bb.bloom_filter, c.byte_pos, c.bit_masks)
          )
    )
    ORDER BY cp_lo ...
    LIMIT ...
)
```

### 4. Fine Filter (Checkpoint Level)

Expand matched blocks to candidate checkpoints, then filter with `cp_blooms`:

```sql
SELECT cb.cp_sequence_number
FROM cp_blooms cb
JOIN candidate_cps cc ON cb.cp_sequence_number = cc.cp_sequence_number
WHERE bloom_contains(cb.bloom_filter, ARRAY[...]::INT[], ARRAY[...]::INT[])
ORDER BY cb.cp_sequence_number ...
LIMIT ...
```

### 5. Load Data

For candidate checkpoints:

1. Load transaction digests from KV store
2. Load transaction contents (for `scanTransactions`) or events (for `eventsScan`)

### 6. Apply Exact Filters

Iterate through loaded data, apply exact filter matching to eliminate bloom filter false positives, and build paginated results.

**Overfetch multiplier:** 1.2x to account for false positives, calculated through analysis of ~2.5 million checkpoints.
