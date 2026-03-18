# Consensus Core Refactoring: Validator and Observer Node Support

## Overview

This document describes the refactoring of the consensus core to support both Validator and Observer node types through the Proposer trait abstraction.

## Motivation

The consensus system needs to support two types of nodes:
- **Validators**: Full consensus participants that propose blocks
- **Observers**: Read-only nodes that process blocks without proposing

The refactoring separates block proposal logic from the core DAG processing logic using trait-based polymorphism.

## Architecture Changes

### 1. Proposer Trait Abstraction

Created a new `Proposer` trait in `src/proposer.rs` that encapsulates all block proposal logic:

```rust
pub(crate) trait Proposer: Send + Sync {
    fn try_new_block(&mut self, force: bool) -> Option<ExtendedBlock>;
    fn should_propose(&self) -> bool;
    fn set_propagation_delay(&mut self, delay: Round);
    fn set_last_known_proposed_round(&mut self, round: Round);
    fn get_last_known_proposed_round(&self) -> Option<Round>;
    fn last_proposed_round(&self) -> Option<Round>;
    fn last_proposed_block(&self) -> Option<VerifiedBlock>;
    fn set_propagation_scores(&mut self, scores: BTreeMap<AuthorityIndex, u64>);
    fn notify_own_blocks_committed(&self, block_refs: Vec<BlockRef>, gc_round: Round);
}
```

### 2. ValidatorProposer Implementation

**File**: `src/proposer.rs`

Full implementation of block proposal logic for validators:

**Key Components**:
- `try_new_block()` - Creates and proposes blocks (260+ lines)
  - Clock round validation
  - Leader existence checks
  - Minimum round delay enforcement
  - Smart ancestor selection
  - Transaction collection and certification
  - Block creation, signing, and serialization
  - Direct DagState acceptance (bypassing BlockManager for own blocks)
  - Transaction acknowledgment
  - Metrics recording

- `should_propose()` - Determines if node should propose
  - Propagation delay checks
  - Amnesia recovery (last_known_proposed_round) validation

- `smart_ancestors_to_propose()` - Intelligent ancestor selection (~250 lines)
  - Propagation score-based selection
  - Quorum enforcement
  - Excluded ancestor handling
  - Earlier ancestor fallback for poorly propagated blocks

**Dependencies**:
```rust
struct ValidatorProposer {
    context: Arc<Context>,
    transaction_consumer: TransactionConsumer,
    transaction_certifier: TransactionCertifier,
    propagation_delay: Round,
    last_included_ancestors: Vec<Option<BlockRef>>,
    block_signer: ProtocolKeyPair,
    last_known_proposed_round: Option<Round>,
    ancestor_state_manager: AncestorStateManager,
    round_tracker: Arc<RwLock<RoundTracker>>,
    dag_state: Arc<RwLock<DagState>>,
    leader_schedule: Arc<LeaderSchedule>,
}
```

### 3. ObserverProposer Implementation

**File**: `src/proposer.rs`

No-op implementation for observer nodes:
- All methods return `None`, `false`, or perform no operations
- Minimal memory footprint

### 4. Core Struct Refactoring

**Before**:
```rust
pub(crate) struct Core {
    // ... many validator-specific fields
    transaction_consumer: TransactionConsumer,
    propagation_delay: Round,
    last_included_ancestors: Vec<Option<BlockRef>>,
    block_signer: ProtocolKeyPair,
    last_known_proposed_round: Option<Round>,
    ancestor_state_manager: AncestorStateManager,
    round_tracker: Arc<RwLock<RoundTracker>>,
    // ...
}
```

**After**:
```rust
pub(crate) struct Core {
    context: Arc<Context>,
    transaction_certifier: TransactionCertifier,
    block_manager: BlockManager,
    committer: UniversalCommitter,
    last_signaled_round: Round,
    last_decided_leader: Slot,
    leader_schedule: Arc<LeaderSchedule>,
    commit_observer: CommitObserver,
    signals: CoreSignals,
    dag_state: Arc<RwLock<DagState>>,
    proposer: Box<dyn Proposer>,  // ← Polymorphic proposer
}
```

### 5. Constructor Separation

**Validator Constructor** (`Core::new_validator()`):
```rust
pub(crate) fn new_validator(
    context: Arc<Context>,
    leader_schedule: Arc<LeaderSchedule>,
    transaction_consumer: TransactionConsumer,
    transaction_certifier: TransactionCertifier,
    block_manager: BlockManager,
    commit_observer: CommitObserver,
    signals: CoreSignals,
    node_type: NodeType,  // Contains ProtocolKeyPair
    dag_state: Arc<RwLock<DagState>>,
    sync_last_known_own_block: bool,
    round_tracker: Arc<RwLock<RoundTracker>>,
) -> Self
```

**Observer Constructor** (`Core::new_observer()`):
```rust
pub(crate) fn new_observer(
    context: Arc<Context>,
    leader_schedule: Arc<LeaderSchedule>,
    transaction_certifier: TransactionCertifier,
    block_manager: BlockManager,
    commit_observer: CommitObserver,
    signals: CoreSignals,
    dag_state: Arc<RwLock<DagState>>,
) -> Self
```

Note: Observer doesn't need `transaction_consumer`, `round_tracker`, or `node_type` since it doesn't propose.

### 6. NodeType Enum

**File**: `src/authority_node.rs`

Made `Clone` to avoid ownership issues:
```rust
#[derive(Clone)]
pub enum NodeType {
    Validator(AuthorityIndex, ProtocolKeyPair),
    Observer,
}
```

## Design Decisions

### Why ValidatorProposer Doesn't Use BlockManager

**Rationale**: Newly created blocks by a validator:
1. Cannot have missing ancestors (we get them from DagState)
2. Cannot have suspended descendants (block is brand new)
3. Are always valid (we created and signed them)

Therefore, ValidatorProposer adds blocks directly to DagState using `accept_block()`, bypassing the BlockManager's suspension logic which is only needed for peer blocks.

### Separation of Concerns

| Component | Responsibility |
|-----------|---------------|
| **ValidatorProposer** | Block creation, ancestor selection, signing, DagState acceptance |
| **ObserverProposer** | No-op (observers don't propose) |
| **Core** | DAG management, commit logic, signal coordination, peer block handling |
| **BlockManager** | Suspending/accepting blocks from peers with missing ancestors |

## Files Modified

1. `src/proposer.rs` - NEW: Proposer trait and implementations (~750 lines)
2. `src/core.rs` - Refactored Core struct and constructors
3. `src/lib.rs` - Added proposer module export
4. `src/authority_node.rs` - Made NodeType Clone
5. `src/context.rs` - Updated to handle NodeType

## Current Status

### ✅ Completed
- [x] Proposer trait definition
- [x] ValidatorProposer full implementation
- [x] ObserverProposer implementation
- [x] Core struct refactoring
- [x] new_validator() and new_observer() constructors
- [x] NodeType made Clone
- [x] All helper methods and dependencies set up

### 🔄 In Progress
- [ ] Refactor Core methods to delegate to proposer
- [ ] Update try_propose() to use proposer.try_new_block()
- [ ] Remove Core's old try_new_block() implementation
- [ ] Update set_propagation_delay() delegation
- [ ] Update commit logic to use proposer.notify_own_blocks_committed()

### 📋 TODO
- [ ] Update all tests to use new constructors
- [ ] Add ValidatorProposer unit tests
- [ ] Add ObserverProposer unit tests
- [ ] Run cargo xclippy and fix warnings
- [ ] Update AuthorityNode to use new Core constructors

## Benefits

1. **Clear Separation**: Block proposal logic cleanly separated from DAG processing
2. **Type Safety**: Compile-time guarantee that observers don't propose
3. **Resource Efficiency**: Observers don't allocate unnecessary proposal-related structures
4. **Testability**: Proposer logic can be tested independently
5. **Maintainability**: Changes to proposal logic isolated to ValidatorProposer

## Next Steps

1. Complete Core delegation to proposer
2. Update tests
3. Run linting and fix warnings
4. Integration testing with both node types
