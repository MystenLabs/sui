# Authenticated Events

Authenticated Events provide a cryptographically verifiable stream of Move events from Sui smart contracts. Unlike regular events, authenticated events can be verified by a light client without trusting any intermediary, making them suitable for applications requiring trustless event consumption.

## Why Authenticated Events?

Regular Sui events are indexed and queryable, but verifying them requires trusting the RPC provider. Authenticated events solve this by:

1. **Cryptographic Commitments**: Events are committed into a Merkle Mountain Range (MMR) structure stored on-chain
2. **Checkpoint Verification**: The commitment is tied to Sui checkpoints, which are signed by the validator committee
3. **Light Client Verification**: Clients verify the chain from genesis committee → checkpoint signatures → event inclusion proofs

This enables cryptographic verification of event completeness and correctness — no full node required, and lightweight enough to run in a browser or on mobile.

## Quick Start

### Emitting Authenticated Events (Move)

```move
module my_package::my_module;

use sui::event;

public struct MyEvent has copy, drop {
    value: u64,
    data: vector<u8>,
}

public entry fun do_something(value: u64) {
    // Emit an authenticated event
    event::emit_authenticated(MyEvent {
        value,
        data: vector::empty(),
    });
}
```

The event is automatically associated with the package that defines the event type. Only that package can emit events to its stream.

### Backwards Compatibility with Events

Authenticated events are fully backwards-compatible with regular Sui events. An authenticated event is simply a regular event with additional metadata for verification — existing event consumers will continue to work unchanged.

To upgrade:
1. **Contract change**: Replace `event::emit(...)` calls with `event::emit_authenticated(...)` in your Move code
2. **Package upgrade**: Deploy the updated package (requires a contract upgrade)
3. **Optional**: Update consumers to use the authenticated events client for cryptographic verification

Existing indexers, explorers, and tools that consume regular events will continue to see authenticated events without modification.

### Consuming Events (Rust Reference Client)

```rust
use sui_light_client::authenticated_events::AuthenticatedEventsClient;
use sui_types::base_types::SuiAddress;
use futures::StreamExt;
use std::sync::Arc;

// Initialize with genesis committee (establishes trust root)
let client = Arc::new(
    AuthenticatedEventsClient::new(rpc_url, genesis_committee)
        .await?
);

// Stream events from your package
let stream_id = SuiAddress::from(package_id);
let mut stream = client.clone().stream_events(stream_id).await?;

while let Some(result) = stream.next().await {
    match result {
        Ok(event) => {
            // event.event contains the verified Move event data
            // event.checkpoint indicates when it was committed
            println!("Verified event at checkpoint {}", event.checkpoint);
        }
        Err(e) => {
            // Transient errors (TransportError, RpcError) are retried automatically.
            // Terminal errors require action:
            // - VerificationError: Data integrity issue, try a different RPC endpoint
            // - InternalError: Invalid state, investigate the cause
            // See "Error Handling" section for details.
            eprintln!("Terminal error: {:?}", e);
            break;
        }
    }
}
```

### Resuming from a Checkpoint

To resume a stream, the client needs a verified starting state. To guarantee completeness, this requires an OCS inclusion proof showing the `EventStreamHead` at that checkpoint — which only exists if authenticated events were emitted for the stream at that checkpoint. In practice, this would be the checkpoint sequence of the last event the client recieved before a disconnection.

If the specified checkpoint does not have events for that stream_id, the client will fail to initialize.

```rust
let last_checkpoint = 12345;
let mut stream = client.clone()
    .stream_events_from_checkpoint(stream_id, last_checkpoint)
    .await?;
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Move Contract                             │
│   event::emit_authenticated(MyEvent { ... })                    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    EventStreamHead (On-chain)                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  mmr: vector<u256>     // Merkle Mountain Range root    │   │
│  │  checkpoint_seq: u64   // Last update checkpoint        │   │
│  │  num_events: u64       // Total events in stream        │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   Light Client Verification                      │
│  1. Fetch EventStreamHead with OCS inclusion proof              │
│  2. Verify proof against checkpoint (signed by committee)       │
│  3. Fetch events from ListAuthenticatedEvents RPC               │
│  4. Recompute MMR from events, compare to EventStreamHead       │
└─────────────────────────────────────────────────────────────────┘
```

### Key Components

| Component | Description |
|-----------|-------------|
| `emit_authenticated<T>()` | Move function to emit verified events |
| `EventStreamHead` | On-chain commitment structure per package |
| `ListAuthenticatedEvents` | gRPC API to fetch events with pagination |
| `AuthenticatedEventsClient` | Reference Rust client for verification |
| MMR (Merkle Mountain Range) | Append-only commitment structure |

### `emit_authenticated` and Accumulator Settlement

Users call `emit_authenticated()` to emit authenticated events. This writes a regular Event alongside metadata including the `stream_id` to Transaction Effects.

Events are batched and settled at checkpoint boundaries through the **accumulator settlement** process:

1. **Event Emission**: During transaction execution, authenticated events are emitted
2. **Consensus Commit Batching**: Authenticated Events are grouped by consensus commit, and a merkle tree is computed over events for each `stream_id`
3. **Checkpoint Settlement**: At each checkpoint, validators execute a settlement transaction that:
   - Appends each consensus commit's merkle root to the stream's MMR
   - Updates the `EventStreamHead` object for that `stream_id` with the new MMR state and event count

## API Reference

### EventService

```protobuf
service EventService {
  rpc ListAuthenticatedEvents(ListAuthenticatedEventsRequest)
      returns (ListAuthenticatedEventsResponse);
}
```

**Request Parameters:**
- `stream_id` (required): Package address that emitted the events
- `start_checkpoint`: Checkpoint to start from (default: 0)
- `page_size`: Events per page (default: 1000, max: 1000)
- `page_token`: Pagination token from previous response

**Response:**
- `events`: List of `AuthenticatedEvent` with checkpoint, transaction index, event index, and payload
- `highest_indexed_checkpoint`: Latest indexed checkpoint
- `next_page_token`: Token for next page (empty if no more events)

### ProofService

```protobuf
service ProofService {
  rpc GetObjectInclusionProof(GetObjectInclusionProofRequest)
      returns (GetObjectInclusionProofResponse);
}
```

Used to verify that the EventStreamHead object exists at a specific checkpoint.

**Request Parameters:**
- `object_id` (required): The EventStreamHead object ID (derived from package address)
- `checkpoint` (required): Checkpoint sequence number to prove inclusion at

**Response:**
- `object_ref`: Object reference (object_id, version, digest)
- `inclusion_proof`: OCS (Object Checkpoint State) merkle proof containing:
  - `merkle_proof`: BCS-encoded proof nodes
  - `leaf_index`: Position in the merkle tree
  - `tree_root`: Root digest (32 bytes)
- `object_data`: BCS-encoded EventStreamHead object
- `checkpoint_summary`: BCS-encoded checkpoint summary for verification

The inclusion proof verifies that the EventStreamHead was written at the specified checkpoint. This is essential for resuming streams - the checkpoint must be one where events were actually emitted.

### Client Configuration

```rust
let config = ClientConfig::new(
    page_size,                    // Events per RPC call (max 1000)
    poll_interval,                // How often to poll for new events
    max_pagination_iterations,    // Max pages before forcing checkpoint boundary
    rpc_timeout,                  // RPC call timeout
)?;

let client = AuthenticatedEventsClient::new_with_config(
    rpc_url,
    genesis_committee,
    config
).await?;
```

## Trust Model

1. **Genesis Committee**: The client is initialized with the genesis validator committee public keys
2. **Trust Ratcheting**: When epochs change, the client verifies the new committee was signed by the previous one
3. **Checkpoint Verification**: Each checkpoint summary is verified against the committee's aggregate signature
4. **Object Inclusion Proof**: The EventStreamHead object is proven to be modified at a specific checkpoint
5. **MMR Verification**: Events are verified against the EventStreamHead's MMR commitment

The client verifies a checkpoint range by:
1. Starting with a verified `EventStreamHead` from the preceding checkpoint (or empty state for a new stream)
2. Fetching events in the range and appending them to the MMR
3. Fetching the `EventStreamHead` at the final checkpoint and proving its inclusion via OCS proof
4. Comparing the locally computed MMR against the on-chain `EventStreamHead`

If any event is missing, modified, or out of order, the computed MMR will not match the on-chain state.

The client automatically handles epoch transitions and committee changes.

## Security Guarantees

**Completeness**: When you receive event N from a stream, you are guaranteed to have received all events 0..N-1 in the correct order. The MMR structure ensures that any missing or reordered events would cause verification to fail.

**What completeness does NOT guarantee**: An intermediary (e.g., RPC provider) is not obligated to return all events up to a requested checkpoint height. To detect withholding, clients must verify against the on-chain `EventStreamHead` which contains the authoritative event count. The reference client does this automatically — if the events received don't match the on-chain commitment, verification fails.

**Correctness**: Each event's content is committed to the MMR. Any modification to event data would cause the computed MMR to diverge from the on-chain state.

## Limitations

- Events are retrievable only if they are indexed on the full node ~ they must not be pruned
- Events must be consumed in checkpoint order
- The EventStreamHead must be updated at the resume checkpoint (authenticated events must have been emitted)
- One stream per package (stream_id = package address)

## Error Handling

| Error Type | Recoverable | Action |
|------------|-------------|--------|
| `TransportError` | Yes | Automatic retry |
| `RpcError` (Unavailable, DeadlineExceeded) | Yes | Automatic retry |
| `VerificationError` | No | Stop - data integrity issue |
| `InternalError` | No | Stop - invalid state |

The stream automatically retries transient errors. Terminal errors indicate verification failures or invalid state and require investigation.
