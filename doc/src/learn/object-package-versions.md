---
title: Object and Package Versioning
---

You reference every object stored on chain by an ID and **version**.  When a transaction modifies an object, it writes the new contents to an on-chain reference with the same ID but a later version. This means that a single object (with ID `I`) might appear in multiple entries in the distributed store:

```
(I, v0) => ...
(I, v1) => ...  # v0 < v1
(I, v2) => ...  # v1 < v2
```

Despite appearing multiple times in the store, only one version of the object is available to transactions -- the latest version (`v2` in the previous example) -- and only one transaction can modify the object at that version to create a new version, guaranteeing a linear history (`v1` was created in a state where `I` was at `v0`, and `v2` was created in a state where `I` was at `v1`).

Versions are strictly increasing and (ID, version) pairs are never re-used. This structure allows node operators to prune their stores of old object versions that are now inaccessible, if they choose. This is not a requirement, though, as node operators might keep prior object versions around to serve requests for an object's history, either from other nodes that are catching up, or from RPC requests.

## Move objects

Sui uses [Lamport timestamps](https://en.wikipedia.org/wiki/Lamport_timestamp) in its versioning algorithm for objects. The use of Lamport timestamps guarantees that versions never get re-used as the new version for objects touched by a transaction is one greater than the max version among all input objects to the transaction.  For example, a transaction transferring an object `O` at version `5` using a gas object `G` at version `3` updates both `O` and `G` versions to `1 + max(5, 3) = 6`. 

The relevance of Lamport versions for maintaining the "no (ID, version) re-use" invariant or for accessing an object as a transaction input changes depending on that object's ownership, as detailed in the following sections.

### Address-owned objects

You must reference address-owned transaction inputs at a specific ID and version. When a validator signs a transaction with an owned object input at a specific version, that version of the object is **locked** to that transaction. Validators reject requests to sign other transactions that require the same input (same ID and version).

If `F + 1` validators sign one transaction that takes an object as input, and a different `F + 1` validators sign a different transaction that takes the same object as input, that object (and all the other inputs to both transactions) are **equivocated** meaning they cannot be used for any further transactions in that epoch.  This is because neither transaction can form a quorum without relying on a signature from a validator that has already committed the object to a different transaction, which it cannot get.  All locks are reset at the end of the epoch, which frees the objects again.

Only an object's owner can equivocate it, but this is not a desirable thing to do.  You can avoid equivocation by carefully managing the versions of address-owned input objects.

### Immutable objects

Like address-owned objects, you reference immutable objects at an ID and version, but they do not need to be locked as their contents and versions do not change. Their version is relevant because they could have started life as an address-owned object before being frozen. The given version identifies the point at which they became immutable.

### Shared objects

Specifying a shared transaction input is slightly more complex. You reference it by its ID, the version it was shared at, and a flag indicating whether it is accessed mutably. You don't specify the precise version the transaction accesses because it is decided by consensus during transaction scheduling. When scheduling multiple transactions that touch the same shared object, validators agree on the order of those transactions, and pick each transaction's input versions for the shared object accordingly (one transaction's output version becomes the next transaction's input version, and so on).

Shared transaction inputs that you reference immutably participate in scheduling, but don't modify the object or increment its version.

### Wrapped objects

The `make_wrapped` function in the following example creates an `Inner` object, wrapped in an `Outer` object, which is sent back to the transaction sender.

```
module example::wrapped {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    
    struct Inner has key, store {
        id: UID,
        x: u64,
    }
    
    struct Outer has key {
        id: UID,
        inner: Inner,
    }
    
    entry fun make_wrapped(ctx: &mut TxContext) {
        let inner = Inner {
            id: object::new(ctx),
            x: 42,
        };

        let outer = Outer {
            id: object::new(ctx),
            inner,
        };

        transfer::transfer(outer, tx_context::sender(ctx));
    }
}
```

You can't access wrapped objects by their ID in the object store, you must access them by the object that wraps them. In the prior example, the owner of `Outer` must specify it as the transaction input and then access its `inner` field to read the instance of `Inner`. Validators refuse to sign transactions that specify wrapped objects (like the `inner` of an `Outer`) as inputs.  As a result, you don't need to specify a wrapped object's version in a transaction that reads that object.

Wrapped objects can eventually become "unwrapped", meaning that they are once again accessible at their ID:

```
module example::wrapped {
    // ...
    
    entry fun unwrap(outer: Outer, ctx: &TxContext) {
        let Outer { id, inner } = outer;
        object::delete(id);
        transfer::transfer(inner, tx_context::sender(ctx));
    }
}
```

The `unwrap` function in the previous code takes an instance of `Outer`, destroys it, and sends the `Inner` back to the sender.  After calling this function, the previous owner of `Outer` can access `Inner` directly by its ID because it is now unwrapped.  Wrapping and unwrapping of an object can happen multiple times across its lifespan, and the object retains its ID across all those events.

The Lamport timestamp-based versioning scheme ensures that the version that an object is unwrapped at is always greater than the version it was wrapped at, to prevent version re-use.

- After a transaction, `W`, where object `I` is wrapped by object `O`, `O`'s version is greater than or equal to `I`'s. This means one of the following conditions is true:
  - `I` is an input so has a strictly lower version.
  - `I` is new and has an equal version.
- After a later transaction unwrapping `I` out of `O`, the following must be true: 
  - `O`'s input version is greater than or equal to its version after `W` because it is a later transaction, so the version can only have increased.
  - `I`'s version in the output must be strictly greater than `O`'s input version.
  
This leads to the following chain of inequalities for `I`'s version before wrapping:

- less than or equal to `O`'s version after wrapping
- less than or equal to `O`'s version before unwrapping
- less than `I`'s version after unwrapping

So `I`'s version before wrapping is less than `I`'s version after unwrapping.

### Dynamic fields

From a versioning perspective, values held in dynamic fields behave like wrapped objects:

- They are only accessible via the field's parent object, not as direct transaction inputs.
- Based on the previous point, you do not need to supply their IDs or versions with the transaction inputs.
- Lamport timestamp-based versioning makes sure that when a field contains an object and a transaction removes that field, its value becomes accessible by its ID and the value's version has been incremented to a previously unused version.

One distinction to wrapped objects is that if a transaction modifies a dynamic object field, its version is incremented in that transaction, where a wrapped object's version would not be.

Adding a new dynamic field to a parent object also creates a `Field` object, responsible for associating the field name and value with that parent. Unlike other newly created objects, the ID for the resulting instance of `Field` is _not_ created using `sui::object::new`. Instead, it is computed as a hash of the parent object ID and the type and value of the field name, so that it can be used to look-up the `Field` via its parent and name.

When a field is removed its associated `Field` is deleted, and if a new field with the same name is added, a new instance will be created, **with the same ID**. Versioning using Lamport timestamps, coupled with the fact that dynamic fields can only be accessed through their parent object ensures that (ID, version) pairs are not reused in the process:

- The transaction that deletes the original field will increment the parent's version to be greater than the deleted field's version.
- The transaction that creates the new version of the same field will create it with a version that is greater than the parent's version.

So the version of the new `Field` instance will be greater than the version of the deleted `Field`.

## Packages

Move packages are also versioned and stored on chain, but follow a different versioning scheme to objects because they are immutable from their inception. This means that you refer to package transaction inputs (e.g. the package that a function is from for a move call transaction) by just their ID, and are always loaded at their latest version.

### User packages

Every time you publish or upgrade a package **a new ID is generated**, a newly published package will have its version set to **1**, whereas an upgraded package's version will be one greater than the package it is upgrading.  Unlike objects, older versions of a package remain accessible even after being upgraded.  For example, imagine a package `P` that is published and upgraded twice.  It might be represented in the store as:

```
(0x17fb7f87e48622257725f584949beac81539a3f4ff864317ad90357c37d82605, 1) => P v1
(0x260f6eeb866c61ab5659f4a89bc0704dd4c51a573c4f4627e40c5bb93d4d500e, 2) => P v2
(0xd24cc3ec3e2877f085bc756337bf73ae6976c38c3d93a0dbaf8004505de980ef, 3) => P v3
```

In the example above, all three versions of the same package are at different IDs. The packages have increasing versions but it is possible to call into v1, even though v2 and v3 exist on-chain.

### Framework packages

Framework packages (such as the Move standard library at `0x1` and the Sui Framework at `0x2`) are a special-case because their IDs must remain stable across upgrades.  The network can upgrade framework packages while preserving their IDs via a system transaction, but can only perform this operation on epoch boundaries because they are considered immutable like other packages.  New versions of framework packages retain the same ID as their predecessor, but increment their version by one:

```
(0x1, 1) => MoveStdlib v1
(0x1, 2) => MoveStdlib v2
(0x1, 3) => MoveStdlib v3
```

The prior example shows the on-chain representation of the first three versions of the Move standard library.

### Package versions

Sui smart contracts are organized into [upgradeable](../build/package-upgrades.md) packages and, as a result, multiple versions of any given package can exist on chain. Before someone can use an on-chain package, you must [publish](../build/move/debug-publish.md#publishing-a-package) its first, original version. When you upgrade a package, you create a new version of that package. Each upgrade of a package is based on the immediately preceding version of that package in the versions history. In other words, you can upgrade the _n_th version of a package from only the _n_th - 1 version. For example, you can upgrade a package from version 1 to 2, but afterwards you can upgrade that package only from version 2 to 3; you're not allowed to upgrade from version 1 to 3.

There is a notion of versioning in package [manifest](../build/move/manifest.md) files, existing in both the [package section](../build/move/manifest.md#package-section) and in the [dependencies section](../build/move/manifest.md#dependencies-section). For example, consider the manifest code that follows:

```toml
[package]
name = "some_pkg"
version = "1.0.0"

[dependencies]
another_pkg = { git = "https://github.com/another_pkg/another_pkg.git" , version = "2.0.0"}
```

At this point, the version references in the manifest are used only for user-level documentation as the `publish` and `upgrade` commands do not leverage this information. If you publish a package with a certain package version in the manifest file and then modify and re-publish the same package with a different version (using `publish` command rather than `upgrade` command), the two are considered different packages, rather than on-chain versions of the same package. You should not use any of these packages as a [dependency override](../build/dependency-overrides.md) to stand in for the other one. While you can specify this type of override when building a package, it results in an error when publishing or upgrading on chain.
