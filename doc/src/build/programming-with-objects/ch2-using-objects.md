---
title: Chapter 2 - Using Objects
---

In [Chapter 1](./ch1-object-basics.md) we covered how to define, create and take ownership of a Sui object in Move. In this chapter we will look at how to use objects that you own in Move calls.

Sui authentication mechanisms ensure only you can use objects owned by you in Move calls. (We will cover non-owned objects in future chapters.) To use an object in Move calls, pass them as parameters to an [entry function](../move.md#entry-functions). Similar to Rust, there are a few ways to pass parameters:

### Pass objects by reference
There are two ways to pass objects by reference: read-only references (`&T`) and mutable references (`&mut T`). Read-only references allow you to read data from the object, while mutable references allow you to mutate the data in the object. Let's try to add a function that would allow us to update one of `ColorObject`'s values with another `ColorObject`'s value. This will exercise using both read-only references and mutable references.

The `ColorObject` we defined in the previous chapter looks like:
```rust
struct ColorObject has key {
    id: VersionedID,
    red: u8,
    green: u8,
    blue: u8,
}
```
Now let's add this function:
```rust
/// Copies the values of `from_object` into `into_object`.
public entry fun copy_into(from_object: &ColorObject, into_object: &mut ColorObject) {
    into_object.red = from_object.red;
    into_object.green = from_object.green;
    into_object.blue = from_object.blue;
}
```
> :bulb: We declared this function with the `entry` modifier to be callable as an entry function from transactions.

In the above function signature, `from_object` can be a read-only reference because we only need to read its fields; conversely, `into_object` must be a mutable reference since we need to mutate it. In order for a transaction to make a call to the `copy_into` function, **the sender of the transaction must be the owner of both of `from_object` and `into_object`**.

> :bulb: Although `from_object` is a read-only reference in this transaction, it is still a mutable object in Sui storage--another transaction could be sent to mutate the object at the same time! To prevent this, Sui must lock any mutable object used as a transaction input, even when it's passed as a read-only reference. In addition, only an object's owner can send a transaction that locks the object.

Let's write a unit test to see how we could interact with multiple objects of the same type in tests.
In the previous chapter, we introduced the `take_owned<T>` API, which takes an object of type `T` from the global storage created by previous transactions. However, what if there are multiple objects of the same type? `take_owned<T>` will no longer be able to tell which one to return. To solve this problem, we need to use two new APIs. The first is `tx_context::last_created_object_id(ctx)`, which returns the ID of the most recent created object. The second is `test_scenario::take_owned_by_id<T>`, which returns an object of type `T` with a specific object ID.
Now let's take a look at the test (`test_copy_into`):
```rust
let owner = @0x1;
let scenario = &mut test_scenario::begin(&owner);
// Create two ColorObjects owned by `owner`, and obtain their IDs.
let (id1, id2) = {
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 255, 255, ctx);
    let id1 = tx_context::last_created_object_id(ctx);
    color_object::create(0, 0, 0, ctx);
    let id2 = tx_context::last_created_object_id(ctx);
    (id1, id2)
};
```
The above code created two objects. Note that right after each call, we make a call to `tx_context::last_created_object_id` to get the ID of the object just created. At the end we have `id1` and `id2` capturing the IDs of the two objects. Next we retrieve both of them and test the `copy_into` method:
```rust
test_scenario::next_tx(scenario, &owner);
{
    let obj1 = test_scenario::take_owned_by_id<ColorObject>(scenario, id1);
    let obj2 = test_scenario::take_owned_by_id<ColorObject>(scenario, id2);
    let (red, green, blue) = color_object::get_color(&obj1);
    assert!(red == 255 && green == 255 && blue == 255, 0);

    let ctx = test_scenario::ctx(scenario);
    color_object::copy_into(&obj2, &mut obj1);
    test_scenario::return_owned(scenario, obj1);
    test_scenario::return_owned(scenario, obj2);
};
```
We used `take_owned_by_id` to take both objects using different IDs. We then used `copy_into` to update `obj1`'s value using `obj2`'s. We can verify that the mutation works:
```rust
test_scenario::next_tx(scenario, &owner);
{
    let obj1 = test_scenario::take_owned_by_id<ColorObject>(scenario, id1);
    let (red, green, blue) = color_object::get_color(&obj1);
    assert!(red == 0 && green == 0 && blue == 0, 0);
    test_scenario::return_owned(scenario, obj1);
}
```

### Pass objects by value
Objects can also be passed by value into an entry function. By doing so, the object is moved out of Sui storage (a.k.a. deleted). It is then up to the Move code to decide where this object should go.

> :books: Since every [Sui object struct type](./ch1-object-basics.md#define-sui-object) must include `VersionedID` as a field, and the [VersionedID struct](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/id.move) does not have the `drop` ability, the Sui object struct type [must not](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#drop) have `drop` ability either. Hence, any Sui object cannot be arbitrarily dropped and must be either consumed (e.g., transferred to another owner) or deleted by [unpacking](https://move-book.com/advanced-topics/struct.html#destructing-structures), as described below.

There are two ways we can deal with a pass-by-value Sui object in Move:

#### Option 1. Delete the object
If the intention is to actually delete the object, we can unpack the object. This can be done only in the module that defined the struct type, due to Move's [privileged struct operations rules](https://github.com/move-language/move/blob/main/language/documentation/book/src/structs-and-resources.md#privileged-struct-operations). Upon unpacking, if any field is also of struct type, recursive unpacking and deletion will be required.

However, the `id` field of a Sui object requires special handling. We must call the following API in the [ID](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/id.move) module to signal Sui that we intend to delete this object:
```rust
public fun delete(versioned_id: VersionedID);
```
Let's define a function in the `ColorObject` module that allows us to delete the object:
```rust
    public entry fun delete(object: ColorObject) {
        let ColorObject { id, red: _, green: _, blue: _ } = object;
        id::delete(id);
    }
```
As we can see, the object is unpacked, generating individual fields. The u8 values are primitive types and can all be dropped. However the `id` cannot be dropped and must be explicitly deleted through the `id::delete` API. At the end of this call, the object will no longer be stored on-chain.

We can add a unit test for it, as well:
```rust
let owner = @0x1;
// Create a ColorObject and transfer it to @owner.
let scenario = &mut test_scenario::begin(&owner);
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 0, 255, ctx);
};
// Delete the ColorObject we just created.
test_scenario::next_tx(scenario, &owner);
{
    let object = test_scenario::take_owned<ColorObject>(scenario);
    color_object::delete(object);
};
// Verify that the object was indeed deleted.
test_scenario::next_tx(scenario, &owner);
{
    assert!(!test_scenario::can_take_owned<ColorObject>(scenario), 0);
}
```
The first part is the same as what we have seen in [Chapter 1](./ch1-object-basics.md#writing-unit-tests), which creates a new `ColorObject` and puts it in the owner's account. The second transaction is what we are testing: retrieve the object from the storage and then delete it. Since the object is deleted, there is no need (in fact, it is impossible) to return it to the storage. The last part of the test checks that the object is indeed no longer in the global storage and hence cannot be retrieved from there.

#### Option 2. Transfer the object
The owner of the object may want to transfer it to another account. To support this, the `ColorObject` module will need to define a `transfer` API:
```rust
public entry fun transfer(object: ColorObject, recipient: address) {
    transfer::transfer(object, recipient)
}
```
>:bulb: One cannot call `transfer::transfer` directly as it is not an `entry` function.

Let's add a test for transferring too. First of all, we create an object in `owner`'s account and then transfer it to a different account `recipient`:
```rust
let owner = @0x1;
// Create a ColorObject and transfer it to @owner.
let scenario = &mut test_scenario::begin(&owner);
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 0, 255, ctx);
};
// Transfer the object to recipient.
let recipient = @0x2;
test_scenario::next_tx(scenario, &owner);
{
    let object = test_scenario::take_owned<ColorObject>(scenario);
    let ctx = test_scenario::ctx(scenario);
    color_object::transfer(object, recipient, ctx);
};
```
Note that in the second transaction, the sender of the transaction should still be `owner`, because only the `owner` can transfer the object that it owns. After the tranfser, we can verify that `owner` no longer owns the object, while `recipient` now owns it:
```rust
// Check that owner no longer owns the object.
test_scenario::next_tx(scenario, &owner);
{
    assert!(!test_scenario::can_take_owned<ColorObject>(scenario), 0);
};
// Check that recipient now owns the object.
test_scenario::next_tx(scenario, &recipient);
{
    assert!(test_scenario::can_take_owned<ColorObject>(scenario), 0);
};
```

### On-chain interactions
Now it's time to try this out on-chain. Assuming you have already followed the instructions in [Chapter 1](./ch1-object-basics.md#on-chain-interactions), you should already have the package published and a new object created.
Now we can try to transfer it to another account address. First let's see what other account addresses you own:
```
$ sui client addresses
```
Since the default current address is the first address, let's pick the second address in the list as the recipient. In my case, I have `0x1416f3d5af469905b0580b9af843ec82d02efd30`. Let's save it for convenience:
```
$ export RECIPIENT=0x1416f3d5af469905b0580b9af843ec82d02efd30
```
Now let's transfer the object to this address:
```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "ColorObject" --function "transfer" --args \"0x$OBJECT\" \"0x$RECIPIENT\"
```
Now let's see what objects the `RECIPIENT` owns:
```
$ sui client objects --address $RECIPIENT
```
We should be able to see that one of the objects in the list is the new `ColorObject`! This means the transfer was successful.

Let's also try to delete this object:
```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "ColorObject" --function "delete" --args \"0x$OBJECT\"
```
Oops. It will error out and complain that the account address is unable to lock the object, which is a valid error because we have already transferred the object away from the original owner.

In order to operate on this object, we need to switch our client address to `$RECIPIENT`:
```
$ sui client switch --address $RECIPIENT
```
And try the deletion again:
```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "ColorObject" --function "delete" --args \"0x$OBJECT\"
```
In the output, you will see in the `Transaction Effects` section a list of deleted objects.
This shows that the object was successfully deleted. If we run this again:
```
$ sui client objects --address $RECIPIENT
```
We will see that this object is no longer there in the address.

Now you know how to pass objects by reference and value and transfer them on-chain.
