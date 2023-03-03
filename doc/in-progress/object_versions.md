# Object Versions

Every object stored on-chain is referenced by an ID and a **version**.  When an object is modified by a transaction, its new contents are written out in the store to a reference with the same ID and a greater version.  This means that a single object (with ID `I`) may appear in multiple entries in the distributed store:

```
(I, v0) => ...
(I, v1) => ...  # v0 < v1
(I, v2) => ...  # v1 < v2
```

Despite appearing multiple times in the store, only one version of the object is available to transactions -- the latest version (`v2` in the example above) -- and only one transaction is able to modify the object at that version to create a new version, guaranteeing a linear history (`v1` was created in a state where `I` was at `v0`, and `v2` was created in a state where `I` was at `v1`).

Versions are strictly increasing and (ID, version) pairs are never re-used so nodes can prune their stores of old object versions that are now inaccessible, but are not required to: Nodes might keep old versions around to serve requests for an object's history, either from other nodes that are catching up, or from RPC requests.

## Move Objects

Move Object versions are calculated using a variant of [Lamport timestamps](https://en.wikipedia.org/wiki/Lamport_timestamp) that guarantees that versions never get re-used in all cases:  The new version for objects touched by a transaction is one greater than the max version among all input objects to the transaction.  For example, a transaction transferring an object `O` at version `5` using a gas object `G` at version `3` updates both `O` and `G`'s version to `1 + max(5, 3) = 6`.

The relevance of versions for accessing an object as a transaction input changes depending on that object's ownership:

### Address-owned Objects

Address-owned transaction inputs must be referenced at a specific ID and version.  When a validator signs a transaction with an owned object input at a specific version, that version of the object is **locked** to that transaction, from the validator's perspective: it rejects requests to sign other transactions that require the same input (same ID and version).

If `F + 1` validators sign one transaction that takes an object as input, and a different `F + 1` validators sign a different transaction that takes the same object as input, that object (and all the other inputs to both transactions) are **equivocated** meaning they cannot be used for any further transactions in that epoch.  This is because neither transaction can form a quorum without relying on a signature from a validator that has already committed the object to a different transaction, which it cannot get.  All locks are reset at the end of the epoch, which frees the objects up once more.

Only an object's owner can equivocate it, but this is not a desirable thing to do.  Equivocation can be avoided by carefully managing the versions of address-owned input objects.

### Immutable Objects

Like Address-owned objects, immutable objects are also referenced at an ID and version, but they do not need to be locked, because their contents and versions do not change.  Their version is relevant because they could have started life as an Address-owned object and later been frozen. The given version identifies the point at which they became immutable.

### Shared Objects

Specifying a shared transaction input is slightly more complex.  It is referenced by its ID, the version it was shared at, and a flag indicating whether it is accessed mutably.  The precise version the transaction will access is **not** specified, because it is decided by consensus during transaction scheduling: When scheduling multiple transactions that touch the same shared object, validators agree the order of those transactions, and pick each transaction's input versions for the shared object's accordingly (one transaction's output version becomes the next transaction's input version, and so on).

Shared transaction inputs that are referenced immutably participate in scheduling, but don't modify the object or increment its version.

### Wrapped Objects

The `make_wrapped` function in the example below creates an `Inner` object, wrapped in an `Outer` object which is sent back to the transaction sender.

```
module example::wrapped {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext}
    
    struct Inner has key {
        id: UID,
        x: u64,
    }
    
    struct Outer has key {
        id: UID,
        inner: Inner,
    }
    
    entry fun make_wrapped(ctx: &mut TxContext) {
        transfer::transfer(
            Outer {
                id: object::new(ctx),
                inner: Inner {
                    id: object::new(ctx),
                    x: 42,
                }
            }
            tx_context::sender(ctx),
        )
    }
}
```

Wrapped object aren't accessible by their ID in the object store, they can only be accessed via the object that wraps them.  In the example above, the owner of `Outer` must specify it as the transaction input and then access its `inner` field to read the instance of `Inner`.  Validators refuse to sign transaction that specify wrapped objects (like the `inner` of an `Outer`) as inputs.  As a result, a wrapped object's version does not need to specified in a transaction that reads it.

Wrapped objects can eventually become "unwrapped", meaning that they are once again accessible at their ID:

```
module example::wrapped {
    // ...
    
    entry fun unwrap(outer: Outer, ctx: &TxContext) {
        let Outer { id, inner } = outer;
        object::delete(id);
        transfer::transfer(inner, tx_context::sender(ctx))
    }
}
```

The `unwrap` function above takes an instance of `Outer`, destroys it and sends the `Inner` inside back to the sender.  After calling this function, the previous owner of `Outer` can access `Inner` directly at by its ID, because it has been unwrapped.  An object can be wrapped and unwrapped multiple times across its lifetime, and retains its ID across all those events.

The Lamport timestamp-based versioning schemes helps to ensure that the version that an object is unwrapped at is always greater than the version it was wrapped at, to prevent version re-use:

- After a transaction, `W` wrapping object `I` in object `O`, `O`'s version is greater than or equal to `I`'s:
  - either `I` is an input so has a strictly lower version, 
  - or is new and has an equal version.
- After a later transaction unwrapping `I` out of `O`, we know that 
  - `O`'s input version is greater than equal to it's version after `W` because it is a later transaction, so the version can only have increased.
  - `I`'s version in the output must be strictly greater than `O`'s input version.
  
This leads to the following chain of inequalities:

- `I`'s version before wrapping
- is less than or equal to `O`'s version after wrapping
- is less than or equal to `O`'s version before unwrapping
- is less than `I`'s version after unwrapping

So `I`'s version before wrapping is less than `I`'s version after unwrapping.

### Dynamic Fields

From a versioning perspective, values held in dynamic fields behave like wrapped objects:

- They are only accessible via their field owner, not as direct transaction inputs.
- Therefore, their versions do not need to be supplied with the transaction inputs.
- Lamport timestamp-based versioning makes sure that when a field is removed and its value becomes accessible by its ID, its version has been incremented.

One distinction to wrapped objects is that if a dynamic object field is modified by a transaction, its version is incremented in that transaction, where a wrapped object's version would not.

## Packages

Move Packages are also stored on-chain and are also versioned, but follow a different versioning scheme to Objects, because they are immutable objects from their inception.  This means that package transaction inputs (e.g. the package that a function is from for a move call transaction) are referred to by just their ID, and are always loaded at their latest version.

## User Packages

Every time a package is published or upgraded **a new ID is generated**, a newly published package will have its version set to **1**, whereas an upgraded package's version will be one greater than the package it is upgrading.  Unlike objects, older versions of a package remain accessible even after they have been upgraded.  For example, imagine a package `P` that is published and upgraded twice.  It might be represented in the store as:

```
(0x17fb7f87e48622257725f584949beac81539a3f4ff864317ad90357c37d82605, 1) => P v1
(0x260f6eeb866c61ab5659f4a89bc0704dd4c51a573c4f4627e40c5bb93d4d500e, 2) => P v2
(0xd24cc3ec3e2877f085bc756337bf73ae6976c38c3d93a0dbaf8004505de980ef, 3) => P v3
```

In the example above, all three versions of the same package are at different IDs, but with increasing versions, and it is possible to call into v1, even though v2 and v3 exist on-chain.

## Framework Packages

Framework packages (such as the Move standard library at `0x1` and the Sui Framework at `0x2`) are a special-case because their IDs must remain stable across upgrades.  The network can upgrade framework packages while preserving their IDs via a system transaction, but can only perform this operation on epoch boundaries because they are considered immutable like other packages.  New versions of framework packages retain the same ID as their predecessor, but increment their version by one:

```
(0x1, 1) => MoveStdlib v1
(0x1, 2) => MoveStdlib v2
(0x1, 3) => MoveStdlib v3
```

The example above shows the on-chain representation of the first three versions of the Move standard library.

