For each pipeline, the indexer will minimally track the `checkpoint_hi_inclusive`, or highest checkpoint where **ALL** data up to that point is committed, and optionally the `reader_lo` and `pruner_hi` if pruning is enabled. These watermarks are particularly crucial for concurrent pipelines to enable out-of-order processing while maintaining data integrity. Both concurrent and sequential pipelines rely on the `checkpoint_hi_inclusive` committer watermark to understand where to resume processing on restarts, while the `reader_lo` and `pruner_hi` define safe lower bounds for reading and pruning operations.

### **Scenario 1: Simple Watermark (No Pruning)**

With pruning disabled, the indexer will only report each pipeline’s committer `checkpoint_hi_inclusive`. Consider the timeline below, where a number of checkpoints are being processed, with some checkpoints committed out of order.

```bash
Checkpoint Processing Timeline:

[1000] [1001] [1002] [1003] [1004] [1005]
  ✓      ✓      ✗      ✓      ✗      ✗
         ^
  checkpoint_hi = 1001

✓ = Committed (all data written)
✗ = Not Committed (processing or failed)
```

Because the `checkpoint_hi_inclusive` is the highest checkpoint where all data up to that point is committed, the `checkpoint_hi_inclusive` is at 1001, even though checkpoint 1003 is committed, because there is still a gap at 1002. The indexer must report the high watermark at 1001, so as to satisfy the guarantee that all data from start to `checkpoint_hi_inclusive` is available.

**Safe Reading Zone:**

```rust
// Once the checkpoint 1002 is committed, we have a safe reading zone up to 1003
[1000] [1001] [1002] [1003] [1004] [1005]
  ✓      ✓      ✓      ✓      ✗       ✓
[---- SAFE TO READ -------]
(start   →   checkpoint_hi_inclusive at 1001)
```

### **Scenario 2: Pruning Enabled**

When a pipeline is configured with a retention policy, pruning is enabled for the pipeline. For example, your table is growing too large, so you want to keep only the **last 4 checkpoints** (retention = 4). This means that the indexer will periodically update `reader_lo` as the difference between `checkpoint_hi_inclusive` and the configured retention. A separate pruning task is responsible for pruning data between `[pruner_hi, reader_lo)`.

```

[998] [999] [1000] [1001] [1002] [1003] [1004] [1005] [1006]
 🗑️    🗑️     ✓      ✓      ✓      ✓      ✗      ✓      ✗
              ^                    ^
       reader_lo = 1000       checkpoint_hi = 1003

🗑️ = Pruned (deleted)
✓ = Committed
✗ = Not Committed
```

**Current Watermarks:**

**checkpoint_hi_inclusive = 1003:**

- All data from start → 1003 is complete (no gaps)
- Cannot advance to 1005 because 1004 is not committed yet (gap)

**reader_lo = 1000:**

- **lowest checkpoint guaranteed to be available**
- Calculated as: reader_lo = checkpoint_hi - retention + 1
- reader_lo = 1003 - 4 + 1 = 1000

**pruner_hi = 999:**

- **Highest checkpoint that has been deleted**
- Checkpoints 998 and 999 were deleted to save space

**Clear Safe Zones:**

```
[998] [999] [1000] [1001] [1002] [1003] [1004] [1005] [1006]
 🗑️    🗑️     ✓      ✓      ✓      ✓      ✗      ✓      ✗

[--PRUNED--][--- Safe Reading Zone ---] [--- Processing ---]
```

### **How Watermarks Progress Over Time**

**Step 1: Checkpoint 1004 completes**

```
[999] [1000] [1001] [1002] [1003] [1004] [1005] [1006] [1007]
 🗑️     ✓      ✓      ✓      ✓      ✓      ✗      ✓      ✗
        ^                           ^
 reader_lo = 1000           checkpoint_hi = 1004 (advanced by 1!)
```

**Step 2: Reader watermark updates** (happens periodically)

```
[999] [1000] [1001] [1002] [1003] [1004] [1005] [1006] [1007]
 🗑️     ✓      ✓      ✓      ✓      ✓      ✗      ✓      ✗
               ^                   ^
        reader_lo = 1001    checkpoint_hi = 1004
        (1004 - 4 + 1 = 1001)
```

**Step 3: Pruner runs** (after safety delay)

```
[999] [1000] [1001] [1002] [1003] [1004] [1005] [1006] [1007]
 🗑️     🗑️     ✓      ✓      ✓      ✓      ✗      ✓      ✗
               ^                   ^
        reader_lo = 1001    checkpoint_hi = 1004

pruner_hi = 1000 (just deleted checkpoint 1000)
```

### **How Watermarks System Enable Safe Pruning**

The watermark system creates a robust data lifecycle management system:

**1. Guaranteed Data Availability**

- reader_lo represents the **lowest checkpoint guaranteed to be available**
- Readers can safely query any checkpoint between `[reader_lo, checkpoint_hi_inclusive]`
  - Note: Checkpoints older than `reader_lo` might still be temporarily available due to an intentional delay that protects in-flights queries from premature data removal.

**2. Automatic Cleanup Process**

- The pipeline frequently cleans unpruned checkpoints in the range `[pruner_hi, reader_lo)`
- This ensures storage doesn't grow indefinitely while maintaining the retention guarantee
- The pruning process runs with a safety delay to avoid race conditions

**3. Perfect Balance**

- **Storage efficiency**: Old data gets automatically deleted
- **Data availability**: Always maintains retention amount of complete data
- **Safety guarantees**: Readers never encounter missing data gaps
- **Performance**: Out-of-order processing maximizes throughput

This watermark system is what makes concurrent pipelines both high-performance and reliable - enabling massive throughput while maintaining strong data availability guarantees and automatic storage management.
