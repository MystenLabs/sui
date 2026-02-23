# SIP-70: PTB Structural Digest

## Summary

Add a single native function `sui::tx_context::structural_digest(&TxContext): vector<u8>` that returns a deterministic, normalized SHA2-256 hash of the current PTB's structure. This enables DAOs, smart accounts, and governance contracts to vote on "what will be executed" and verify at execution time that the PTB matches the approved template.

## Motivation

### The Problem

DAOs and smart accounts need to vote on a transaction before it is executed. But committing to a PTB is surprisingly hard:

- **Object IDs are brittle** — `SplitCoin` creates new IDs each time
- **`tx_context::digest()` changes every execution** — it hashes the full transaction including signatures, gas payment, and exact object references
- **Hot potato wrappers don't scale** — every protocol integration needs custom wrapper code

### The Solution

Hash the **structure and argument provenance**, not the runtime identity. A structural digest captures:
- Which commands call which targets, in what order
- How results flow between commands (`Result(n)` / `NestedResult(n, m)`)
- What values are passed (Pure bytes, shared object IDs, owned object refs)
- Type arguments for each call

Two PTBs that differ only in which specific coin objects are used (but have the same type, balance, and flow) can produce the same digest.

## Specification

### Normalization Rules

Each PTB argument type is normalized before hashing:

| Argument Type | What Gets Hashed | Rationale |
|---|---|---|
| `GasCoin` | `0x00` marker byte | Gas coin ID is sender-dependent |
| `Input(n)` → `CallArg::Pure(bytes)` | `0x01` + BCS bytes | Exact parameter values |
| `Input(n)` → `ObjectArg::SharedObject` | `0x02` + ObjectID | Static anchors (pools, governance objects) |
| `Input(n)` → `ObjectArg::ImmOrOwnedObject` | `0x03` + ObjectID + version | Identity-stable for non-fungible objects |
| `Input(n)` → `ObjectArg::Receiving` | `0x04` + ObjectID + version | Receiving objects |
| `Result(n)` | `0x05` + command_index(u16) | Provenance — which command produced this |
| `NestedResult(n, m)` | `0x06` + command_index(u16) + result_index(u16) | Provenance with tuple element |

### Hash Construction

```
structural_digest = SHA2-256(
    for each command in order:
        SHA2-256(command_discriminator || command_specific_data || normalized_arguments)
)
```

For `Command::MoveCall`:
```
SHA2-256(0x00 || package_id || module_name || function_name || type_args_bcs || normalized_args)
```

For `Command::TransferObjects`:
```
SHA2-256(0x01 || normalized_objects || normalized_recipient)
```

For `Command::SplitCoins`:
```
SHA2-256(0x02 || normalized_coin || normalized_amounts)
```

For `Command::MergeCoins`:
```
SHA2-256(0x03 || normalized_target || normalized_sources)
```

For `Command::Publish`:
```
SHA2-256(0x04 || module_bytes || dependency_ids)
```

For `Command::MakeMoveVec`:
```
SHA2-256(0x05 || type_tag_bcs || normalized_elements)
```

For `Command::Upgrade`:
```
SHA2-256(0x06 || module_bytes || dependency_ids || package_id || normalized_ticket)
```

### Move API

```move
module sui::tx_context {
    /// Returns the normalized structural digest of the current PTB.
    /// Deterministic: same logical PTB structure -> same digest,
    /// even if coin object IDs differ due to splits/merges.
    public fun structural_digest(_self: &TxContext): vector<u8> {
        native_structural_digest()
    }
    native fun native_structural_digest(): vector<u8>;
}
```

### Governance Example

```move
module dao::executor {
    use sui::tx_context;

    public fun execute_approved(ctx: &TxContext, proposal: &Proposal) {
        let digest = tx_context::structural_digest(ctx);
        assert!(proposal.approved_digest() == digest, EDigestMismatch);
    }
}
```

### Client-Side Computation

The SDK must implement the same normalization algorithm for off-chain digest preview:

```typescript
import { Transaction } from '@mysten/sui/transactions';

const tx = new Transaction();
// ... build PTB ...
const digest = tx.computeStructuralDigest();
// Submit digest to DAO for approval vote
```

## Implementation

### Files Changed

1. **`crates/sui-types/src/transaction.rs`** — `ProgrammableTransaction::structural_digest()` method
2. **`crates/sui-types/src/base_types.rs`** — `structural_digest` field on `TxContext`
3. **`sui-execution/latest/sui-adapter/src/execution_engine.rs`** — Compute digest before PTB execution
4. **`sui-execution/latest/sui-move-natives/src/transaction_context.rs`** — Accessor
5. **`sui-execution/latest/sui-move-natives/src/tx_context.rs`** — Native function impl
6. **`sui-execution/latest/sui-move-natives/src/lib.rs`** — Registration + cost params
7. **`crates/sui-framework/packages/sui-framework/sources/tx_context.move`** — Move declaration
8. **`crates/sui-protocol-config/src/lib.rs`** — Feature gate + gas cost

### Security Considerations

- **Read-only, VM-computed, unforgeable** — contracts cannot influence the computation
- **Order-sensitive** — adding/removing/reordering commands changes the digest
- **Flow-sensitive** — redirecting a `Result` to a different command changes the digest
- **No censorship vector** — contracts cannot inspect individual commands

### Backwards Compatibility

Purely additive. One new native function gated behind a protocol version bump.

## Future Work

- **Coin normalization** — Hash coins by `TypeName + Balance` instead of ObjectID, making the digest stable across coin split/merge. Requires input object resolution during digest computation.
- **Client SDK** — `computeStructuralDigest()` in `@mysten/sui/transactions`
- **Wildcard slots** — Allow certain argument positions to be "any value" in the digest template
