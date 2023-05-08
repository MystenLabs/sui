---
title: Use Sui Move Library
---

Sui provides a list of Sui Move library functions that enables manipulation of objects in Sui. You can view source code for the implementation of the core Sui Move framework in the [Sui GitHub repo](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/).

## Object ownership

Objects in Sui can have different ownership types:
- Exclusively owned by an address.
- Exclusively owned by another object.
- Immutable.
- Shared.

### Owned by an address

The [`Transfer`](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/transfer.move) module provides all the APIs needed to manipulate the ownership of objects.

The most common case is to transfer an object to an address. For example, when you create a new object, you typically transfer it to an address for ownership. In Sui Move, to transfer an object `obj` to an address `recipient`, you import the module then make the transfer:

```rust
use sui::transfer;

transfer::transfer(obj, recipient);
```

This call fully consumes the object, making it no longer accessible in the current transaction. After an address owns an object, for any future use (either read or write) of this object, the signer of the transaction must be the owner of the object.

### Owned by another object

An object can be owned by another object when you add the former as a [dynamic object field](../programming-with-objects/ch5-dynamic-fields.md) of the latter. While external tools can read the dynamic object field value at its original ID, from Move's perspective, you can only access it through the field on its owner using the `dynamic_object_field` APIs:

```rust
use sui::dynamic_object_field as ofield;

let a: &mut A = /* ... */;
let b: B = /* ... */;

// Adds `b` as a dynamic object field to `a` with "name" `0: u8`.
ofield::add<u8, B>(&mut a.id, 0, b);

// Get access to `b` at its new position
let b: &B = ofield::borrow<u8, B>(&a.id, 0);
```

If you pass the value of a dynamic object field as an input to an entry function in a transaction, that transaction fails. For instance, if you have a chain of ownership: address `Addr1` owns object `a`, object `a` has a dynamic object field containing object `b`, and `b` has a dynamic object field containing object `c`, then in order to use object `c` in a Move call, `Addr1` must sign the transaction and accept `a` as an input, and you must access `b` and `c` dynamically during transaction execution:

```
use sui::dynamic_object_field as ofield;

// Signer of ctx is Addr1
public entry fun entry_function(a: &A, ctx: &mut TxContext) {
  let b: &B = ofield::borrow<u8, B>(&a.id, 0);
  let c: &C = ofield::borrow<u8, C>(&b.id, 0);
}
```

You can find more examples of how objects can be transferred and owned in
[object_owner.move](https://github.com/MystenLabs/sui/blob/main/crates/sui-core/src/unit_tests/data/object_owner/sources/object_owner.move).

### Immutable

To make an object `obj` immutable, call `freeze_object`:

```
transfer::freeze_object(obj);
```

After this call, `obj` becomes immutable, meaning you can't mutate or delete it. This process is also irreversible: once an object is frozen, it stays frozen forever. Anyone can use an immutable object as a reference in their Move call.

### Shared

To make an object `obj` shared, call `share_object`:

```
transfer::share_object(obj);
```

After this call, `obj` stays mutable, but becomes shared by everyone so that anyone can send a transaction to mutate this object. However, you cannot transfer or embed a shared object in another object as a field. For more details, see the [shared objects](../../learn/objects.md#shared) documentation.

## Transaction context

The `TxContext` module provides a few important APIs that operate based on the current transaction context.

To create a new ID for a new object:

```
// Assume `ctx` has type `&mut TxContext`.
let info = sui::object::new(ctx);
```

To obtain the current transaction sender's address:

```
sui::tx_context::sender(ctx)
```

## Next steps

Now that you are familiar with the Move language and the Sui Move dialect, as well as with how to develop and test Sui Move code, you are ready to start learning from larger
[examples](../../explore/examples.md) of Move programs. The examples include implementations of Tic Tac Toe and (Hero), a more
developed variant of the fantasy game developed in this tutorial.
