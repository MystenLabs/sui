---
title: Use Sui Move Library
---

Sui provides a list of Move library functions that allows us to manipulate objects in Sui.

## Object ownership
Objects in Sui can have different ownership types. Specifically, they are:
- Exclusively owned by an address.
- Exclusively owned by another object.
- Shared and immutable.
- Shared and mutable (work-in-progress).

### Transfer to address
The [`Transfer`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move) module provides all the APIs needed to manipulate the ownership of objects.

The most common case is to transfer an object to an address. For example, when a new object is created, it is typically transferred to an address so that the address owns the object. To transfer an object `obj` to an address `recipient`:
```
use sui::transfer;

transfer::transfer(obj, recipient);
```
This call will fully consume the object, making it no longer accessible in the current transaction.
Once an address owns an object, for any future use (either read or write) of this object, the signer of the transaction must be the owner of the object.

### Freeze an object
To make an object `obj` shared and immutable, one can call:
```
transfer::freeze_object(obj);
```
After this call, `obj` becomes immutable which means it can never be mutated or deleted. This process is also irreversible: once an object is frozen, it will stay frozen forever. An immutable object can be used as reference by anyone in their Move call.

### Share an object
To make an object `obj` shared and mutable, one can call:
```
transfer::share_object(obj);
```

After this call, `obj` stays mutable, but becomes shared by everyone, i.e. anyone can send a transaction to mutate this object. However, such an object cannot be transferred or embedded in another object as a field. For more details, see the [shared objects](../../learn/objects.md#shared) documentation.

## Transaction context
`TxContext` module provides a few important APIs that operate based on the current transaction context.

To create a new ID for a new object:
```
// assmue `ctx` has type `&mut TxContext`.
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
programs. The examples include implementation of the tic-tac-toe game, and a more
developed variant of a fantasy game similar to the one we have been
developing during this tutorial.
