---
title: Use Sui Move Library
---

Sui provides a list of Move library functions that allows us to manipulate objects in Sui.
You can view source code for the implementation of the core Sui Move framework in the [Sui GitHub repo](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/sources).

## Object ownership
Objects in Sui can have different ownership types. Specifically, they are:
- Exclusively owned by an address.
- Exclusively owned by another object.
- Immutable.
- Shared.

### Owned by an address
The [`Transfer`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move) module provides all the APIs needed to manipulate the ownership of objects.

The most common case is to transfer an object to an address. For example, when a new object is created, it is typically transferred to an address so that the address owns the object. To transfer an object `obj` to an address `recipient`:
```
use sui::transfer;

transfer::transfer(obj, recipient);
```
This call will fully consume the object, making it no longer accessible in the current transaction.
Once an address owns an object, for any future use (either read or write) of this object, the signer of the transaction must be the owner of the object.

### Owned by another object

An object can be owned by another object when the former is added as a [dynamic object field](../programming-with-objects/ch5-dynamic-fields.md) of the latter. While external tools can read the dynamic object field value at its original ID, from Move's perspective, it can only be accessed through the field on its owner using the `dynamic_object_field` APIs:

```
use sui::dynamic_object_field as ofield;

let a: &mut A = /* ... */;
let b: B = /* ... */;

// Adds `b` as a dynamic object field to `a` with "name" `0: u8`.
ofield::add<u8, B>(&mut a.id, 0, b);

// Get access to `b` at its new position
let b: &B = ofield::borrow<u8, B>(&a.id, 0);
```

If the value of a dynamic object field is passed as an input to an entry function in a transaction, that transaction will fail. For instance, if we have a chain of ownership: address `Addr1` owns object `a`, object `a` has a dynamic object field containing object `b`, and `b` has a dynamic object field containing object `c` then in order to use object `c` in a Move call, the transaction must be signed by `Addr1`, and accept `a` as an input, and `b` and `c` must be accessed dynamically during transaction execution:

```
use sui::dynamic_object_field as ofield;

// signer of ctx is Addr1
public entry fun entry_function(a: &A, ctx: &mut TxContext) {
  let b: &B = ofield::borrow<u8, B>(&a.id, 0);
  let c: &C = ofield::borrow<u8, C>(&b.id, 0);
}
```

More examples of how objects can be transferred and owned can be found in
[object_owner.move](https://github.com/MystenLabs/sui/blob/main/crates/sui-core/src/unit_tests/data/object_owner/sources/object_owner.move).

### Immutable
To make an object `obj` immutable, one can call:
```
transfer::freeze_object(obj);
```

After this call, `obj` becomes immutable, meaning you can't mutate or delete it. This process is also irreversible: once an object is frozen, it stays frozen forever. Anyone can use an immutable object as a reference in their Move call.

### Shared
To make an object `obj` shared, one can call:
```
transfer::share_object(obj);
```

After this call, `obj` stays mutable, but becomes shared by everyone, i.e. anyone can send a transaction to mutate this object. However, a shared object cannot be transferred or embedded in another object as a field. For more details, see the [shared objects](../../learn/objects.md#shared) documentation.

## Transaction context
The `TxContext` module provides a few important APIs that operate based on the current transaction context.

To create a new ID for a new object:
```
// assume `ctx` has type `&mut TxContext`.
let info = sui::object::new(ctx);
```

To obtain the current transaction sender's address:
```
sui::tx_context::sender(ctx)
```

## Next steps
Now that you are familiar with the Move language, as well as with how
to develop and test Move code, you are ready to start looking at and
playing with some larger
[examples](../../explore/examples.md) of Move
programs. The examples include implementations of the tic-tac-toe game, and (Hero) a more
developed variant of a fantasy game similar to the one we have been
developing during this tutorial.
