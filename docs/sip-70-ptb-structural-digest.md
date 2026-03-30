# SIP-70: PTB Structural Digest

## Summary

One native function on `sui::tx_context`:

1. `structural_digest(&TxContext): vector<u8>` — returns a versioned, deterministic hash of the current PTB's structure.

This enables DAOs, smart accounts, and governance contracts to approve "what will be executed" and verify at execution time that the PTB matches that approval.

## Motivation

### The Problem

Sui has strong atomic composition, but Move does not have a native way to verify that the currently executing PTB matches a prior approval.

Today, applications that need separated authorization and execution end up building custom wrapper or interpreter logic around PTBs.

### The Goal

Expose a minimal, VM-computed commitment to PTB structure that Move can compare on-chain.

This proposal intentionally does **not** expose full PTB introspection and does **not** define a higher-level authorization model. It only exposes a read-only structural commitment.

## Specification

### Output Format

```
[version_byte | blake2b256_hash]
```

- Version `0x01` = current scheme
- Total output: 33 bytes (1 + 32)

### Normalization Rules

Each PTB argument type is normalized before hashing:

| Argument Type | Discriminator | What Gets Hashed | Rationale |
|---|---|---|---|
| `GasCoin` | `0x00` | marker only | Gas coin ID is sender-dependent |
| `Pure(bytes)` | `0x01` | BCS bytes | Exact parameter values |
| `SharedObject` | `0x02` | ObjectID + mutability mode | Version-independent anchor plus lock semantics |
| `ImmOrOwnedObject` | `0x03` | ObjectID | Version dropped — drifts between approval and execution |
| `Receiving` | `0x04` | ObjectID | Version dropped |
| `Result(n)` | `0x05` | command_index | Provenance, not runtime identity |
| `NestedResult(n,m)` | `0x06` | cmd_index + result_index | Provenance with tuple element |

All strings and byte blobs are length-prefixed with `u32 LE`. All lists are count-prefixed with `u32 LE`. This prevents concatenation collisions in the hash input.

### Hash Construction

```
digest = 0x01 || Blake2b256(
    for each command in order:
        Blake2b256(
            command_discriminator
            || length_framed(command_specific_data)
            || count(arguments) || normalized_arguments
        )
)
```

### Move API

```move
module sui::tx_context {
    /// Returns the structural digest of the current PTB.
    /// Output: [0x01 | blake2b256_hash] (33 bytes).
    public fun structural_digest(_self: &TxContext): vector<u8>;
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

## Implementation

### Files Changed

1. **`crates/sui-types/src/transaction.rs`** — `structural_digest()` algorithm and `StructuralDigestData`
2. **`crates/sui-types/src/base_types.rs`** — `TxContext` storage and lazy digest computation
3. **`sui-execution/latest/sui-adapter/src/execution_engine.rs`** — stores `StructuralDigestData` on `TxContext`
4. **`sui-execution/latest/sui-move-natives/src/transaction_context.rs`** — `structural_digest()` accessor
5. **`sui-execution/latest/sui-move-natives/src/tx_context.rs`** — native implementation
6. **`sui-execution/latest/sui-move-natives/src/lib.rs`** — registration and cost params
7. **`crates/sui-framework/packages/sui-framework/sources/tx_context.move`** — Move declaration
8. **`crates/sui-protocol-config/src/lib.rs`** — feature gate and base/per-byte gas costs (protocol v113)

### Design Decisions

1. **Version prefix (0x01):** Future changes to the digest scheme bump this byte. Contracts storing digests can check `digest[0]` for compatibility.
2. **Owned object version dropped:** Version increments between approval and execution. Hashing by ObjectID only keeps the digest stable across this gap.
3. **Shared object mutability preserved:** Shared object version is dropped for stability, but mutability mode is hashed alongside the ObjectID because immutable, mutable, and non-exclusive-write accesses imply different lock and execution semantics.
4. **Lazy computation:** The PT is stored transiently on `TxContext`, and the digest itself is computed only if requested.
5. **Size-sensitive gas:** The native charges a base cost plus a per-byte charge derived from PTB size so digest work scales with transaction size.

## Security Considerations

- **Read-only, VM-computed, unforgeable** — contracts cannot influence the computation
- **Order-sensitive** — adding, removing, or reordering commands changes the digest
- **Flow-sensitive** — redirecting a `Result` to a different command changes the digest
- **No PTB introspection** — contracts receive only an opaque commitment, not the full transaction structure

## Backwards Compatibility

Purely additive. One new native function gated behind a protocol version bump. The version prefix byte ensures future digest versions are distinguishable.

## Future Work

- Client SDK support for off-chain digest computation
