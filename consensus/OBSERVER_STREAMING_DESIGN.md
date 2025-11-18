# Observer Node Streaming Design

This document captures the design discussion for improving observer node synchronization with validators through block and commit streaming.

## Problem Statement

Observer nodes need to efficiently synchronize with validators. The key challenges are:

1. **Catch-up efficiency**: When an observer falls behind, it needs to catch up quickly
2. **Real-time updates**: Once caught up, it needs low-latency updates
3. **Overlap handling**: During catch-up, there's a transition zone where both commits and blocks are needed
4. **Backpressure**: Observers may not be able to keep up with the data rate

## Design Overview

### Two Parallel Streams

The design uses two separate bi-directional gRPC streams:

1. **Block Stream**: For streaming individual blocks (low latency, real-time)
2. **Commit Stream**: For streaming committed sub-DAGs (efficient catch-up)

### Why Two Streams?

| Aspect | Single Interleaved Stream | Two Parallel Streams |
|--------|---------------------------|----------------------|
| Separation of concerns | Mixed | Clean |
| Flow control | Shared | Independent |
| Overlap handling | Complex interleaving | Run both simultaneously |
| Debugging | Harder | Easier |

The two-stream approach makes the overlap problem trivial - just run both streams during catch-up.

## Subscriber-Controlled Flow

The subscriber (observer node) controls the streaming behavior, not the publisher (validator). This is the better design because:

### 1. Principle of Locality

The subscriber has the most accurate information about:
- Its own processing capacity
- Current queue depths
- Local resource constraints
- How fast it's making progress

### 2. Simpler Publisher = Better Scalability

Publisher just responds to commands - no complex per-subscriber decision logic. This scales better to 20+ subscribers.

### 3. Natural Backpressure

The subscriber can implement backpressure based on actual capacity:
- "I have 500 blocks queued, I can only process 100/sec"
- Action: Pause blocks until queue drains

### 4. Clear Failure Modes

- Subscriber explicitly tells publisher what it wants
- If publisher doesn't hear from subscriber, stop sending

## State Machine

```rust
enum SubscriberMode {
    // Normal operation - streaming blocks only
    BlocksOnly,

    // Catching up - streaming commits, blocks paused
    CommitsOnly,

    // Transition zone - both active
    Overlap,
}
```

### Mode Transitions

```
Time    Subscriber State              Action                    Mode
─────────────────────────────────────────────────────────────────────────
T0      commit_lag=0, blocks ok       (initial)                 BlocksOnly
T1      commit_lag=50                 Start commits, Stop blocks CommitsOnly
T2      commit_lag=10                 Start blocks              Overlap
T3      commit_lag=0                  Stop commits              BlocksOnly
T4      block_processing_lag=500      Pause blocks              (paused)
T5      block_processing_lag=50       Resume blocks             BlocksOnly
```

### Thresholds

```rust
struct SubscriberConfig {
    // When to start commit sync
    start_commit_sync_lag: u64,        // e.g., 50 commits behind

    // When to resume blocks during commit sync
    resume_blocks_lag: u64,            // e.g., 10 commits behind

    // When to stop commit sync
    stop_commit_sync_lag: u64,         // e.g., 0 (fully caught up)

    // Block processing backpressure
    pause_blocks_queue_size: usize,    // e.g., 500 pending blocks
    resume_blocks_queue_size: usize,   // e.g., 50 pending blocks
}
```

## Batching Strategy

To improve efficiency, responses can contain batches of items rather than single items. The batch size can be adaptive based on lag:

```rust
let batch_size = if lag > 1000 {
    100  // Far behind: large batches for efficiency
} else if lag > 100 {
    20   // Moderate lag: medium batches
} else if lag > 10 {
    5    // Small lag: small batches
} else {
    1    // Nearly caught up: single items for low latency
};
```

### Benefits of Batched Streaming

1. **Simpler than hybrid polling/streaming**: One mechanism
2. **Adaptive**: Batch size adjusts to situation
3. **Efficient catch-up**: Large batches when behind
4. **Low latency real-time**: Small batches when caught up
5. **Still push-based**: Publisher sends when data available

## Protocol Buffer Definitions

### Block Stream

```protobuf
service BlockStreamService {
    rpc StreamBlocks(stream BlockStreamRequest) returns (stream BlockStreamResponse);
}

// Subscriber -> Publisher
message BlockStreamRequest {
    oneof command {
        StartBlockStream start = 1;
        StopBlockStream stop = 2;
    }
}

message StartBlockStream {
    // The highest received round per authority (authorities in committee order)
    // Index in the vector corresponds to authority index
    repeated uint64 highest_round_per_authority = 1;
}

message StopBlockStream {}

// Publisher -> Subscriber
message BlockStreamResponse {
    // Batch of serialized blocks
    repeated bytes blocks = 1;

    // The publisher's highest commit index
    uint64 highest_commit_index = 2;
}
```

### Commit Stream

```protobuf
service CommitStreamService {
    rpc StreamCommits(stream CommitStreamRequest) returns (stream CommitStreamResponse);
}

// Subscriber -> Publisher
message CommitStreamRequest {
    oneof command {
        StartCommitStream start = 1;
        StopCommitStream stop = 2;
    }
}

message StartCommitStream {
    // Last commit index the subscriber has processed
    uint64 last_commit_index = 1;
}

message StopCommitStream {}

// Publisher -> Subscriber
message CommitStreamResponse {
    // Batch of committed sub-DAGs
    repeated CommittedSubDagData commits = 1;

    // Publisher's highest commit index
    uint64 highest_commit_index = 2;
}

// Common types
message CommittedSubDagData {
    uint64 commit_index = 1;
    BlockRef leader = 2;
    repeated bytes blocks = 3;
    uint64 timestamp_ms = 4;
}

message BlockRef {
    uint32 authority_index = 1;
    uint64 round = 2;
    bytes digest = 3;
}
```

## Corresponding Rust Structures

```rust
pub(crate) struct BlockStreamRequest {
    command: BlockStreamCommand
}

enum BlockStreamCommand {
    // Instruct the publisher to start sending blocks to the subscriber.
    Start {
        // The highest received BlockRefs per authority (authorities in committee order)
        highest_round_per_authority: Vec<Round>
    },
    // Instruct the publisher to stop sending blocks to the subscriber.
    // This is useful to save bandwidth when subscriber is behind and tries to catch
    // up via commit stream
    Stop
}

pub(crate) struct BlockStreamResponse {
    blocks: Vec<Block>,
    // The publisher's highest commit index. This might be unnecessary as we could extract
    // this information via the block commit votes, but equally is not terribly bad
    highest_commit_index: CommitIndex
}
```

## Publisher Overhead Considerations

With multiple subscribers (10-20+), the main concerns are:

1. **CPU overhead from serialization**: Mitigated by caching serialized blocks (already stored serialized in DB)
2. **Memory for send buffers**: Each stream needs buffered data
3. **Task scheduling**: One task per subscriber stream

### Optimization: Broadcast Pattern

For future optimization, consider serializing once and broadcasting to all subscribers:

```rust
struct BlockBroadcaster {
    // Pre-serialized blocks ready to send
    serialized_cache: LruCache<BlockRef, Bytes>,

    // All active subscribers
    subscribers: Vec<SubscriberHandle>,
}
```

## Checkpoint Certification Context

With consensus committing every ~75ms and `min_checkpoint_interval_ms = 200ms`:

- **Sub-DAGs per checkpoint**: ~3 (batched by time interval)
- **With randomness enabled**: 2 pending checkpoints per sub-DAG (regular + randomness)
- **Checkpoint certification latency (P50)**: ~75-80ms (1 consensus round for signature propagation)
- **Checkpoint certification latency (P99)**: ~150-160ms (2 consensus rounds)

This context is important because observer nodes need to track checkpoint certification to know when they're caught up.

## Implementation Notes

1. **Block storage**: Blocks are already stored as serialized in DB, so sending `bytes` is efficient
2. **Subscriber evaluation**: Periodic evaluation of lag to decide mode transitions
3. **Graceful degradation**: If subscriber can't keep up with commits either, disconnect and retry
4. **Multiple validators**: Currently single target validator; future work could add multi-validator support

## Open Questions

1. Should subscribers be able to hint their preferred batch size?
2. How should randomness signatures be integrated with this streaming design?
3. Should there be a maximum batch size to prevent memory issues?
