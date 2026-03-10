# SIP-70: PTB Structural Digest (v2)

## Summary

Two native functions on `sui::tx_context`:

1. `structural_digest(&TxContext): vector<u8>` — returns a versioned, deterministic hash of the current PTB's structure. Identity-preserving: coins hash by ObjectID.
2. `structural_digest_masked(&TxContext, wildcard_pure_indices: vector<u64>): vector<u8>` — same structure hash, but with coin normalization (TypeTag + balance instead of ObjectID) and specified Pure inputs treated as wildcards.

This enables DAOs, smart accounts, and governance contracts to vote on "what will be executed" and verify at execution time that the PTB matches the approved template.

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
- What values are passed (Pure bytes, shared object IDs, owned object IDs)
- Type arguments for each call
- Two modes: identity-preserving (coins by ObjectID) or normalized (coins by TypeTag + balance)

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
| `SharedObject` | `0x02` | ObjectID | Version-independent anchor |
| `ImmOrOwnedObject` | `0x03` | ObjectID | Version dropped — drifts between vote and execution |
| `Receiving` | `0x04` | ObjectID | Version dropped |
| `Result(n)` | `0x05` | command_index | Provenance, not runtime identity |
| `NestedResult(n,m)` | `0x06` | cmd_index + result_index | Provenance with tuple element |
| `Coin (normalized)` | `0x08` | TypeTag BCS + balance LE | Fungible across split/merge |
| `Coin Receiving (normalized)` | `0x09` | TypeTag BCS + balance LE | Fungible receiving coin |
| `Wildcard Pure` | `0xFF` | marker only | Executor-discretion parameter |

**Coin normalization (masked variant only):** `structural_digest_masked` resolves each object input from the loaded objects. If it's a `Coin<T>`, it hashes by TypeTag + balance (discriminator `0x08`/`0x09`) instead of ObjectID. This makes the digest stable across coin split/merge. The base `structural_digest` always hashes coins by ObjectID (`0x03`/`0x04`), preserving identity for use cases where specific coin objects matter.

**Why two modes:** Coin identity is deliberately erased in the masked variant because governance approves economic intent ("transfer 100 SUI"), not specific UTXOs. However, some protocols key on coin ObjectID (deposit receipts, position tracking). The base variant preserves identity for these cases. Use `structural_digest` when coin identity matters, `structural_digest_masked(vector[])` for fungible coin handling.

**Wildcard slots:** When `structural_digest_masked` is called with wildcard indices, Pure inputs at those positions are hashed as `0xFF` instead of their actual bytes. This lets a DAO approve "swap on this DEX at this amount" while leaving slippage tolerance to the executor. Wildcard indices are validated: values exceeding `u16::MAX` cause an abort.

**Length framing:** All variable-length fields are length-prefixed (u32 LE) and all lists are count-prefixed to prevent concatenation collisions (e.g. module "a" + function "bc" vs module "ab" + function "c").

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

All strings and byte blobs are prefixed with their length as u32 LE. All lists are prefixed with their count as u32 LE. This prevents boundary ambiguity in the hash input.

### Move API

```move
module sui::tx_context {
    /// Returns the structural digest of the current PTB.
    /// Identity-preserving: coins hash by ObjectID.
    /// Output: [0x01 | blake2b256_hash] (33 bytes).
    public fun structural_digest(_self: &TxContext): vector<u8>;

    /// Returns the structural digest with coin normalization and optional wildcards.
    /// Coins hash by TypeTag + balance (fungible across split/merge).
    /// Wildcarded Pure inputs hash as 0xFF marker instead of their value.
    /// `wildcard_pure_indices` contains input indices to wildcard (pass empty for none).
    public fun structural_digest_masked(
        _self: &TxContext,
        wildcard_pure_indices: vector<u64>,
    ): vector<u8>;
}
```

### Governance Example

```move
module dao::executor {
    use sui::tx_context;

    /// Verify exact PTB structure matches what was approved.
    public fun execute_approved(ctx: &TxContext, proposal: &Proposal) {
        let digest = tx_context::structural_digest(ctx);
        assert!(proposal.approved_digest() == digest, EDigestMismatch);
    }

    /// Verify PTB structure with executor-discretion parameters.
    /// Proposal stores: approved_digest + wildcard_indices.
    public fun execute_with_wildcards(ctx: &TxContext, proposal: &Proposal) {
        let digest = tx_context::structural_digest_masked(
            ctx,
            proposal.wildcard_indices(),
        );
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

1. **`crates/sui-types/src/transaction.rs`** — `structural_digest()` + `structural_digest_with_options()` algorithm, `StructuralDigestData` type
2. **`crates/sui-types/src/base_types.rs`** — `structural_digest` + `structural_digest_data` fields on `TxContext`, `structural_digest_masked()` method
3. **`sui-execution/latest/sui-adapter/src/execution_engine.rs`** — `build_coin_info_map()` resolves coins from input objects, stores `StructuralDigestData` on TxContext
4. **`sui-execution/latest/sui-move-natives/src/transaction_context.rs`** — `structural_digest_masked()` accessor
5. **`sui-execution/latest/sui-move-natives/src/tx_context.rs`** — Two native function impls
6. **`sui-execution/latest/sui-move-natives/src/lib.rs`** — Registration + cost params for both natives
7. **`crates/sui-framework/packages/sui-framework/sources/tx_context.move`** — Move declarations for both functions
8. **`crates/sui-protocol-config/src/lib.rs`** — Feature gate + gas costs (protocol v113)

### Design Decisions

1. **Version prefix (0x01):** Future changes to the digest scheme bump this byte. Contracts storing digests can check `digest[0]` for compatibility.

2. **Owned object version dropped:** Version increments between proposal vote and execution. Hashing by ObjectID only (same as shared objects) makes the digest stable across this gap.

3. **Two-mode coin handling:** `structural_digest` preserves coin identity (ObjectID). `structural_digest_masked` normalizes coins to TypeTag + balance. This is because coin normalization erases object identity, which is correct for governance ("transfer 100 SUI to Bob") but incorrect for protocols that key on coin ObjectID. The base variant is the safe default; the masked variant opts into fungibility.

4. **Coin normalization at execution engine level:** The `ProgrammableTransaction` struct doesn't carry object types/balances. The execution engine resolves coins from the loaded input objects (and receiving objects from the backing store) and passes a `coin_info` map to the digest function.

5. **Wildcard recomputation:** The full PT + coin_info is stored (transient, `#[serde(skip)]`) on TxContext so `structural_digest_masked` can recompute with wildcard substitution at runtime.

6. **Length framing:** All variable-length fields are u32 LE length-prefixed before hashing to prevent concatenation collisions across field boundaries.

7. **Wildcard validation:** Wildcard indices are validated at the native layer. Values exceeding u16::MAX cause an abort to prevent silent truncation (e.g. 65536 aliasing index 0).

### Security Considerations

- **Read-only, VM-computed, unforgeable** — contracts cannot influence the computation
- **Order-sensitive** — adding/removing/reordering commands changes the digest
- **Flow-sensitive** — redirecting a `Result` to a different command changes the digest
- **No censorship vector** — contracts cannot inspect individual commands
- **Wildcards are caller-specified** — the contract decides which indices are wildcards, not the executor

### Backwards Compatibility

Purely additive. Two new native functions gated behind a protocol version bump. The version prefix byte ensures old and new digests are distinguishable.

## Future Work

- **Client SDK** — `computeStructuralDigest()` / `computeStructuralDigestMasked()` in `@mysten/sui/transactions`
