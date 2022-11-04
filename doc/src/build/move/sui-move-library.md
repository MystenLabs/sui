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

### Transfer to object
We can also transfer an object to be owned by another object. Note that the ownership is only tracked in Sui. From Move's perspective, these two objects are still more or less independent, in that the child object isn't part of the parent object in terms of data store.
Once an object is owned by another object, it is required that for any such object referenced in the entry function, its owner must also be one of the argument objects. For instance, if we have a chain of ownership: address `Addr1` owns object `a`, object `a` owns object `b`, and `b` owns object `c`, in order to use object `c` in a Move call, the entry function must also include both `b` and `a`, and the signer of the transaction must be `Addr1`, like this:
```
// signer of ctx is Addr1.
public entry fun entry_function(a: &A, b: &B, c: &mut C, ctx: &mut TxContext);
```

A common pattern of object owning another object is to have a field in the parent object to track the ID of the child object. It is important to ensure that we keep such a field's value consistent with the actual ownership relationship. For example, we do not end up in a situation where the parent's child field contains an ID pointing to object A, while in fact the parent owns object B. To ensure the consistency, we defined a custom type called `ChildRef` to represent object ownership. Whenever an object is transferred to another object, a `ChildRef` instance is created to uniquely identify the ownership. The library implementation ensures that the `ChildRef` goes side-by-side with the child object so that we never lose track or mix up objects.
To transfer an object `obj` (whose owner is an address) to another object `owner`:
```
transfer::transfer_to_object(obj, &mut owner);
```
This function returns a `ChildRef` instance that cannot be dropped arbitrarily. It can be stored in the parent as a field.
Sometimes we need to set the child field of a parent while constructing it. In this case, we don't yet have a parent object to transfer into. In this case, we can call the `transfer_to_object_id` API. Example:
```
let parent_info = object::new(ctx);
let child = Child { info: object::new(ctx) };
let (parent_id, child_ref) = transfer::transfer_to_object_id(child, parent_info);
let parent = Parent {
    info: parent_info,
    child: child_ref,
};
transfer::transfer(parent, tx_context::sender(ctx));
```
To transfer an object `child` from one parent object to a new parent object `new_parent`, we can use the following API:
```
transfer::transfer_child_to_object(child, child_ref, &mut new_parent);
```
Note that in this call, we must also have the `child_ref` to prove the original ownership. The call will return a new instance of `ChildRef` that the new parent can maintain.
To transfer an object `child` from an object to an address `recipient`, we can use the following API:
```
transfer::transfer_child_to_address(child, child_ref, recipient);
```
This call also requires to have the `child_ref` as proof of original ownership.
After this transfer, the object will be owned by `recipient`.

More examples of how objects can be transferred and owned can be found in
[object_owner.move](https://github.com/MystenLabs/sui/blob/main/crates/sui-core/src/unit_tests/data/object_owner/sources/object_owner.move).

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
