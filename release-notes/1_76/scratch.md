# `sui::scratch`: A Per-Transaction Scratch Pad

Sui v1.76 introduces `sui::scratch`, an ephemeral, per-transaction key-value store. Think of it as a "scratch pad" you can write to for the duration of a transaction: all entries persist throughout the PTB (across commands), but are dropped at the end of the transaction.

It may be helpful to think of it like adding dynamic fields directly to `TxContext`. However, the entries are not persisted with any object, and every entry is dropped before the transaction ends. Nothing written to scratch outlives the transaction, and nothing is charged for storage.

## The API

Each entry is identified by the pair of its key _type_ and key _value_, hashed together the same way as a dynamic field name (see `sui::dynamic_field::hash_type_and_key`). So `WrappedU8(1)` and `WrappedBool(true)` are distinct keys even though they serialize to the same bytes, because the key type is part of the derived address.

The core operations mirror `sui::dynamic_field`, and are exposed as methods on `TxContext`:

- `add(key, value)` inserts an entry. Aborts if the key is already present.
- `read(key)` returns a copy of the value. The entry stays in place.
- `remove(key)` removes the entry and returns its value. Aborts if the key is not present.
- `exists_with_type(key)` and `exists(key)` test for presence, with and without checking the type of the value respectively.
- Additional helper functions and macros will be rolled out over the coming releases.

## Use Cases

We foresee a common use of scratch being to constrain how a function is called _within a single PTB_. A few examples of per-PTB limits:

- Enforce that a function is called at most once (or up to any limit you choose) across the whole transaction.
- Enforce that a function is called at most once for a given set of object inputs.
- Enforce that a function is called at most once for a given set of type parameters.

For instance, a DEX could use scratch to allow at most one swap from token A to token B per PTB, keying an entry on the ordered `(A, B)` type pair and aborting on the second attempt. Because scratch is cleared at the end of every transaction, the next transaction starts fresh. There is no lingering state to reset or clean up.

### Example

A minimal "call at most once per PTB" guard looks like this:

```move
module example::once;

use sui::tx_context::TxContext;

#[error]
const EAlreadyCalled: vector<u8> = "Function has already been called in this PTB";

// A key type this module owns.
public struct Called() has copy, drop;

public fun do_thing(ctx: &mut TxContext /* ... */) {
    // Abort if we have already run in this PTB; otherwise mark that we have.
    assert!(!ctx.scratch_internal_exists!<Called>(Called()), EAlreadyCalled);
    ctx.scratch_internal_add!<Called, bool>(Called(), true);

    // ... rest of the logic ...
}
```

## A note on access control

Scratch entries are namespaced by their key type, and only the module that defines the key type `K` can access the entries keyed by it. Within the defining module, the `scratch_internal_*` macros used above handle the access control for you: under the hood they issue a `sui::scratch::Permit<K>` from a `std::internal::Permit<K>`, which can only be constructed by `K`'s defining module.
For more granular control or for accessing entries outside of the defining module, you will need to pass the `sui::scratch::Permit<K>` explicitly. The non-macro forms (`scratch_add`, `scratch_read`, and so on) take a `Permit<K>` as an argument.
