# Informal Specification: `verify_sender_signed_data_message_signatures`

**File**: `crates/sui-types/src/signature_verification.rs:130`

---

## Definitions

- **System transactions** have no meaningful signatures and are unconditionally valid.
- **User transactions** are all non-system transactions. The remainder of this spec applies to user transactions only.
- **`senders(T)`** — the ordered sequence `[S1, S2, ...]` of one or more sender addresses for transaction `T`.
- **`signatures(T)`** — the ordered sequence `[Z1, Z2, ...]` of signatures accompanying `T`. Duplicate signature values are permitted (each occupies a distinct position).
- **`addresses(Z)`** — the non-empty finite set of addresses derivable from signature `Z` (e.g. a zklogin signature with legacy address support yields both a legacy and a canonical address). Address derivation may fail for malformed signatures; see the validity condition below.
- **`aliases(S)`** — the non-empty finite set of signature addresses that are valid for sender `S`. Typical case: `aliases(S) = {S}`. Any finite set is permitted.
- **`is_valid_for(Z, S)`** — signature `Z` is valid for sender `S` iff:
  - There exists an address `A` such that `A ∈ addresses(Z) ∧ A ∈ aliases(S)`, **and**
  - `Z` is a cryptographically valid proof of authorization by `A` (i.e. verification runs against the matching address `A`, not against the sender `S` directly).

---

## Precondition: No Address Collisions

For any well-formed transaction, the addresses of distinct signatures must be disjoint:

> `∀ Z1, Z2 ∈ signatures(T). Z1 ≠ Z2 → addresses(Z1) ∩ addresses(Z2) = {}`

Without this constraint, a greedy assignment algorithm could fail to find a valid bijection even when one exists. For example:

- `senders(T) = [A1, A2]`, `signatures(T) = [Z1, Z2]`
- `addresses(Z1) = {A1, A2}`, `addresses(Z2) = {A1}`
- A valid bijection exists: `A1 → Z2, A2 → Z1`
- But a greedy algorithm assigning A1 first would pick Z1 (valid for A1), leaving no valid signature for A2

This precondition is expected to hold by construction of valid signature types; it is not checked by this function.

---

## Validity Condition

A transaction `T` is **invalid** (returns `Err`) if any of the following hold:

1. The transaction's intent is not `SUI_TRANSACTION_INTENT`.
2. Any signature `Z ∈ signatures(T)` has an uncomputable address set (address derivation fails).
3. `T` is a user transaction and `|senders(T)| ≠ |signatures(T)|`.
4. `T` is a user transaction and the greedy algorithm below fails for any sender.

A **system transaction** `T` is otherwise **valid** unconditionally: it returns `Ok([0, 1, 2, ..., n-1])` where `n = |senders(T)|`.

A **user transaction** `T` is **valid** iff the following greedy algorithm succeeds for every sender:

> For each sender `S` in `senders(T)` in order:  
> assign `M(S)` to be the **first unused** position `j` in `signatures(T)` such that `is_valid_for(signatures(T)[j], S)`.  
> If no such position exists, the transaction is **invalid**.

In English: every sender must be matched to a valid signature, in sender order, consuming each signature position at most once.

### Example with duplicate signatures

Suppose:
- `senders(T) = [S1, S2]`
- `aliases(S1) = {S1}`, `aliases(S2) = {S1}`

Then `signatures(T)` must contain two positions, both holding a signature `Z` where `is_valid_for(Z, S1)` holds (i.e. `S1 ∈ addresses(Z)`). The greedy algorithm assigns position 0 to S1 and position 1 to S2. A single-element `signatures(T) = [Z]` would be invalid: after assigning position 0 to S1, no unused position remains for S2.

---

## Return Value

`verify_sender_signed_data_message_signatures` returns `SuiResult<Vec<u8>>`.

- **`Err(_)`**: `T` is invalid (any condition in the validity section above).
- **`Ok([0, 1, ..., n-1])`** where `n = |senders(T)|`: `T` is a valid system transaction. The returned indices are sequential and do not represent meaningful signature assignments.
- **`Ok(indices)`**: `T` is a valid user transaction. `indices` is a sequence parallel to `senders(T)` where `indices[k]` is the position in `signatures(T)` assigned to `senders(T)[k]` by the greedy algorithm.
