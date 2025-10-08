# Consensus Development Context

## Current Work
**Observer Nodes Feature** - Adding functionality for non-committee nodes (observers/full nodes) to stream blocks from consensus validators.

### Context:
- Observer nodes are NOT part of the Committee and don't have an AuthorityIndex
- They should be able to subscribe to block streams from validators
- We're modifying NetworkService to support this use case

### Changes Made So Far:
1. Added `NodeId` enum in `core/src/network/mod.rs:60-93` that can represent either:
   - `Authority(AuthorityIndex)` - committee member
   - `Observer(NetworkPublicKey)` - observer node identified by public key

2. Added observer port configuration in `config/src/parameters.rs`:
   - `TonicParameters.observer_port_offset: Option<u16>` (default: Some(1000))
   - Observer port = validator_port + offset
   - Set to None to disable observer network

3. Added `ObserverConsensusService` gRPC service in `core/build.rs`:
   - Separate service definition with only `subscribe_blocks` method
   - Generates both client and server stubs
   - Same request/response types as ConsensusService but limited surface

4. Implemented observer server in `TonicManager` (`core/src/network/tonic_network.rs`):
   - Added `validator_server` and `observer_server` fields to `TonicManager`
   - Observer server starts on `validator_port + observer_port_offset` if configured
   - Created `ObserverServiceProxy` that implements `ObserverConsensusService` trait
   - Created `ObserverPeerInfo` struct to hold observer's `NetworkPublicKey`
   - Added `observer_peer_info_from_certs()` to extract public key from TLS certs
   - Observer server accepts any valid TLS certificate (not just committee members)
   - Both servers properly shut down in `stop()` and `Drop`

5. Implemented observer client (`core/src/network/tonic_network.rs`):
   - Created `ObserverClient` as a **standalone specialized client** (does NOT implement `NetworkClient`)
   - Only has one public method: `subscribe_blocks(peer, last_received, timeout) -> BlockStream`
   - Created `ObserverChannelPool` to manage connections to validator observer ports
   - Connects to `validator_port + observer_port_offset`
   - Uses TLS with client certificate authentication
   - User agent: "mysticeti-observer" for tracking
   - Simpler interface: no need to stub out validator-only methods

6. Modified `NetworkService::handle_subscribe_blocks()` to accept `NodeId`:
   - Changed signature from `peer: AuthorityIndex` to `peer: NodeId`
   - Updated `TonicServiceProxy` to pass `NodeId::Authority(peer_index)` for validators
   - Updated `ObserverServiceProxy` to pass `NodeId::Observer(peer_public_key)` for observers
   - Updated `BroadcastStream` in `authority_service.rs` to handle `NodeId`

7. Extended metrics to track observer subscriptions separately:
   - Added "node_type" label to `subscribed_by` metric (values: "validator" or "observer")
   - Updated `SubscriptionCounter` to track observer subscriptions separately
   - Added `increment_node()` and `decrement_node()` methods to handle `NodeId`
   - Validators tracked individually by hostname with node_type="validator"
   - Observers tracked collectively as "observers" with node_type="observer"
   - Both validator and observer subscriptions contribute to `set_subscriber_exists()` dispatcher call

8. Modified Core to support observer mode (2025-10-07):
   - Made `block_signer: Option<ProtocolKeyPair>` in Core struct
   - Updated `Core::new()` signature to accept `Option<ProtocolKeyPair>`
   - Added check in `should_propose()` - returns false when `block_signer.is_none()`
   - Observers have full Core functionality (process blocks, update DAG, participate in commits)
   - Observers cannot propose new blocks (no block_signer)
   - Updated all test callsites to pass `Some(block_signer)`
   - AuthorityNode passes `protocol_keypair` (already Optional) directly to Core

## Recent Changes
<!-- Track recent commits and changes -->
- 2b533f826e: Add serialising macro for node config
- 77d18eba30: Allow mfp validator submission list
- 34bb447a30: Add ping label on client monitor metrics

## Active Branch
- feature/observer-nodes

## Modified Files
- core/src/network/mod.rs (added NodeId enum, removed Hash derive, included ObserverConsensusService, changed handle_subscribe_blocks signature)
- config/src/parameters.rs (added observer_port_offset)
- core/build.rs (added ObserverConsensusService generation)
- core/src/network/tonic_network.rs (added ObserverClient, ObserverChannelPool, ObserverServiceProxy, dual server support, NodeId usage)
- core/src/authority_service.rs (updated handle_subscribe_blocks, BroadcastStream, SubscriptionCounter to use NodeId)
- core/src/metrics.rs (added node_type label to subscribed_by metric)
- core/src/authority_node.rs (made own_index and protocol_keypair Optional parameters, pass protocol_keypair to Core)
- core/src/core.rs (made block_signer Optional, updated should_propose() to check for block_signer, updated all test callsites)

## Next Steps
1. ~~Modify `NetworkService::handle_subscribe_blocks()` to accept `NodeId` instead of `AuthorityIndex`~~ ✅ DONE
2. ~~Update network implementations (anemo_network, tonic_network) to handle observer connections~~ ✅ DONE
3. ~~Add authentication/authorization for observer nodes~~ ✅ DONE
4. ~~Update metrics to track observer connections separately~~ ✅ DONE
5. ~~Make Core support observer mode (optional protocol_keypair, don't propose blocks)~~ ✅ DONE
6. **NEXT**: Test the observer functionality:
   - Create integration test with observer node
   - Verify observer can connect and stream blocks
   - Verify observer doesn't propose blocks
   - Verify observer processes received blocks correctly

## Design Decisions

### Observer Network Architecture (Proposed)

**Goal:** Allow Full Nodes (observers) to stream blocks from validators without being in the committee.

**Approach:** Single `TonicManager` that can spawn multiple servers based on node type:

#### Server Configuration

**Validators run TWO servers:**
1. **Validator Server** (existing port from committee config)
   - Accepts: Committee members only (authenticated via TLS + AuthorityIndex lookup)
   - RPC Methods: Full surface (send_block, fetch_blocks, fetch_commits, subscribe_blocks, etc.)
   - Uses: `TonicServiceProxy<S>` with `ConsensusService` trait

2. **Observer Server** (NEW port, separate config)
   - Accepts: Observer nodes (authenticated via TLS + NetworkPublicKey)
   - RPC Methods: ONLY `subscribe_blocks` (read-only streaming)
   - Uses: NEW `ObserverServiceProxy<S>` with NEW `ObserverConsensusService` trait

**Full Nodes run ONE server:**
- Observer Server on observer port
- Accepts: Other full nodes for peer-to-peer block sharing
- RPC Methods: ONLY `subscribe_blocks`

#### Implementation Plan

1. **Create new gRPC service definition in `build.rs`:**
   ```rust
   // New ObserverConsensusService with only subscribe_blocks method
   ```

2. **Add observer port configuration:**
   - Add `observer_port` to consensus config
   - Conditionally read based on node type

3. **Modify `TonicManager`:**
   ```rust
   pub(crate) struct TonicManager {
       validator_server: Option<ServerHandle>,  // only if validator
       observer_server: Option<ServerHandle>,   // always present
       client: Arc<TonicClient>,
   }
   ```

4. **Create `ObserverServiceProxy` and `ObserverConsensusService`:**
   - Similar to `TonicServiceProxy` but only implements `subscribe_blocks`
   - Uses `NodeId::Observer(NetworkPublicKey)` instead of `AuthorityIndex`
   - TLS verification accepts any public key (not just committee members)

5. **Shared components:**
   - `commit_syncer.rs` and `synchronizer.rs` continue using `NetworkClient` trait
   - Work with both validator and observer networks transparently

#### Key Changes Needed

**Files to modify:**
- `core/build.rs` - Add `ObserverConsensusService` generation
- `core/src/network/tonic_network.rs` - Add observer server logic to `TonicManager`
- Config files - Add `observer_port` configuration

**Files to create:**
- Observer service proxy implementation
- Observer-specific TLS verification (accept non-committee keys)

## Known Issues
<!-- Track bugs or problems to fix -->

## Notes
<!-- Any other relevant information -->

### Important Clarification (2025-10-07)
**Observers DO have a Core!** The initial assumption that observers wouldn't have Core was incorrect.

**Correct design:**
- Observers **DO** have a Core to process received streamed blocks
- Observers **DO NOT** propose new blocks (no block_signer/protocol_keypair)
- Core's `should_propose()` will return false for observers
- Core's `block_signer` field should be Optional for observers
- All other components (Synchronizer, CommitSyncer, etc.) are needed for block processing

**Why observers need Core:**
- To process and validate blocks received from validators
- To update DAG state with received blocks
- To participate in commit processing
- To maintain consensus state for serving other full nodes

---
Last updated: 2025-10-07
