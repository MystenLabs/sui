---
title: Chapter 2 - Using Objects
---

The [Object Basics](./ch1-object-basics.md) chapter covered how to define, create and take ownership of a Sui object in Sui Move. This chapter describes how to use objects that you own in Sui Move calls.

Sui authentication mechanisms ensure only you can use objects owned by you in Sui Move calls. To use an object in Sui Move calls, pass them as parameters to an [entry function](../move/index.md#entry-functions). Similar to Rust, there are a few ways to pass parameters, as described in the following sections.

### Pass objects by reference

There are two ways to pass objects by reference: read-only references (`&T`) and mutable references (`&mut T`). Read-only references allow you to read data from the object, while mutable references allow you to mutate the data in the object. To add a function that allows you to update one of the values of `ColorObject` with another value of `ColorObject`. This exercises both using read-only references and mutable references.

The `ColorObject` defined in the previous chapter looks like:
```rust
struct ColorObject has key {
    id: UID,
    red: u8,
    green: u8,
    blue: u8,
}
```

Now, add this function:

```rust
/// Copies the values of `from_object` into `into_object`.
public entry fun copy_into(from_object: &ColorObject, into_object: &mut ColorObject) {
    into_object.red = from_object.red;
    into_object.green = from_object.green;
    into_object.blue = from_object.blue;
}
```

Declare this function with the `entry` modifier so that it is callable as an entry function from transactions.

In the preceding function signature, `from_object` can be a read-only reference because you only need to read its fields. Conversely, `into_object` must be a mutable reference since you need to mutate it. For a transaction to make a call to the `copy_into` function, the sender of the transaction must be the owner of both `from_object` and `into_object`.

Although `from_object` is a read-only reference in this transaction, it is still a mutable object in Sui storage--another transaction could be sent to mutate the object at the same time. To prevent this, Sui must lock any mutable object used as a transaction input, even when it's passed as a read-only reference. In addition, only an object's owner can send a transaction that locks the object.

The following section describes how to write a unit test to learn how to interact with multiple objects of the same type in tests.

The previous chapter introduced the `take_from_sender<T>` function, which takes an object of type `T` from the global storage created by previous transactions. However, if there are multiple objects of the same type, `take_from_sender<T>` is no longer able to determine which one to return. To solve this problem, use two new, test-only functions. The first is `tx_context::last_created_object_id(ctx)`, which returns the ID of the most recently created object. The second is `test_scenario::take_from_sender_by_id<T>`, which returns an object of type `T` with a specific object ID.

Create the test `test_copy_into` as shown in this example:

```rust
let owner = @0x1;
let scenario_val = test_scenario::begin(owner);
let scenario = &mut scenario_val;
// Create two ColorObjects owned by `owner`, and obtain their IDs.
let (id1, id2) = {
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 255, 255, ctx);
    let id1 = object::id_from_address(tx_context::last_created_object_id(ctx));
    color_object::create(0, 0, 0, ctx);
    let id2 = object::id_from_address(tx_context::last_created_object_id(ctx));
    (id1, id2)
};
```

The preceding code creates two objects. Note that right after each call, it makes a call to `tx_context::last_created_object_id` to get the ID of the object the call created. At the end, `id1` and `id2` capture the IDs of the two objects. Next, retrieve both of them and test the `copy_into` function:

```rust
test_scenario::next_tx(scenario, owner);
{
    let obj1 = test_scenario::take_from_sender_by_id<ColorObject>(scenario, id1);
    let obj2 = test_scenario::take_from_sender_by_id<ColorObject>(scenario, id2);
    let (red, green, blue) = color_object::get_color(&obj1);
    assert!(red == 255 && green == 255 && blue == 255, 0);

    let ctx = test_scenario::ctx(scenario);
    color_object::copy_into(&obj2, &mut obj1);
    test_scenario::return_to_sender(scenario, obj1);
    test_scenario::return_to_sender(scenario, obj2);
};
```

This uses `take_from_sender_by_id` to take both objects using different IDs. Use `copy_into` to update the value for `obj1` using the value for `obj2`. You can verify that the mutation works:

```rust
test_scenario::next_tx(scenario, owner);
{
    let obj1 = test_scenario::take_from_sender_by_id<ColorObject>(scenario, id1);
    let (red, green, blue) = color_object::get_color(&obj1);
    assert!(red == 0 && green == 0 && blue == 0, 0);
    test_scenario::return_to_sender(scenario, obj1);
};
test_scenario::end(scenario_val);
```

### Pass objects by value

You can also pass objects by value into an entry function. By doing so, the object is moved out of Sui storage. It is then up to the Sui Move code to decide where this object should go.

Since every [Sui object struct type](./ch1-object-basics.md#define-sui-object) must include `UID` as its first field, and the [UID struct](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/object.move) does not have the `drop` ability, the Sui object struct type cannot have the [drop](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#drop) ability either. Hence, any Sui object cannot be arbitrarily dropped and must be either consumed (for example, transferred to another owner) or deleted by [unpacking](https://move-book.com/advanced-topics/struct.html#destructing-structures), as described in the following sections.

There are two ways to handle a pass-by-value Sui object in Move:
 * delete the object
 * transfer the object

#### Delete the object

If the intention is to actually delete the object, unpack it. You can do this only in the module that defined the struct type, due to Move's [privileged struct operations rules](https://github.com/move-language/move/blob/main/language/documentation/book/src/structs-and-resources.md#privileged-struct-operations). If any field is also of struct type, you must use recursive unpacking and deletion when you unpack the object.

However, the `id` field of a Sui object requires special handling. You must call the following API in the [object](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/object.move) module to signal Sui that you intend to delete this object:

```rust
public fun delete(id: UID) { ... }
```

Define a function in the `ColorObject` module that allows us to delete the object:

```rust
    public entry fun delete(object: ColorObject) {
        let ColorObject { id, red: _, green: _, blue: _ } = object;
        object::delete(id);
    }
```

The object unpacks and generates individual fields. You can drop all of the u8 values, which are primitive types. However, you can't drop the `id`, which has type `UID`, and must explicitly delete it using the `object::delete` API. At the end of this call, the object is no longer stored on-chain.

To add a unit test for it:

```rust
let owner = @0x1;
// Create a ColorObject and transfer it to @owner.
let scenario_val = test_scenario::begin(owner);
let scenario = &mut scenario_val;
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 0, 255, ctx);
};
// Delete the ColorObject just created.
test_scenario::next_tx(scenario, owner);
{
    let object = test_scenario::take_from_sender<ColorObject>(scenario);
    color_object::delete(object);
};
// Verify that the object was indeed deleted.
test_scenario::next_tx(scenario, &owner);
{
    assert!(!test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
test_scenario::end(scenario_val);
```

The first part of the example repeats the example used in [Object Basics](./ch1-object-basics.md#writing-unit-tests), and creates a new `ColorObject` and puts it in the owner's account. The second transaction gets tested. It retrieves the object from the storage and then delete it. Since the object is deleted, there is no need (in fact, it is impossible) to return it to the storage. The last part of the test checks that the object is indeed no longer in the global storage and hence cannot be retrieved from there.

#### Option 2. Transfer the object

The owner of the object might want to transfer it to another address. To support this, the `ColorObject` module needs to define a `transfer` function:

```rust
public entry fun transfer(object: ColorObject, recipient: address) {
    transfer::transfer(object, recipient)
}
```

You cannot call `transfer::transfer` directly as it is not an `entry` function.

Add a test for transferring too. First, create an object in the account of the `owner`, and then transfer it to a different account `recipient`:

```rust
let owner = @0x1;
// Create a ColorObject and transfer it to @owner.
let scenario_val = test_scenario::begin(owner);
let scenario = &mut scenario_val;
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 0, 255, ctx);
};
// Transfer the object to recipient.
let recipient = @0x2;
test_scenario::next_tx(scenario, owner);
{
    let object = test_scenario::take_from_sender<ColorObject>(scenario);
    let ctx = test_scenario::ctx(scenario);
    transfer::transfer(object, recipient, ctx);
};
```

Note that in the second transaction, the sender of the transaction should still be `owner`, because only the `owner` can transfer the object that it owns. After the transfer, you can verify that `owner` no longer owns the object, and `recipient` now owns it:

```rust
// Check that owner no longer owns the object.
test_scenario::next_tx(scenario, owner);
{
    assert!(!test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
// Check that recipient now owns the object.
test_scenario::next_tx(scenario, recipient);
{
    assert!(test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
test_scenario::end(scenario_val);
```

### On-chain interactions

Next, try this out on-chain. Assuming you followed the instructions in [Object Basics](./ch1-object-basics.md#on-chain-interactions), you should have a published package and a new object created.

To transfer it to another address, first check the addresses available:

```shell
sui client addresses
```

Choose an address other than the active address. If you have only one address, create another address using the [Sui Client CLI](../cli-client.md#create-a-new-account-address).

For this example, the recipient address is: `0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446`. Save it as a variable for convenience:

```shell
export RECIPIENT=0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446
```

Now, transfer the object to the address:

```shell
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "transfer" --args \"$OBJECT\" \"$RECIPIENT\"
```

Now let's see what objects the `RECIPIENT` owns:

```shell
$ sui client objects $RECIPIENT
```

You should see the `ColorObject` listed. This means the transfer succeeded.

To delete this object:

```shell
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "delete" --args \"$OBJECT\"
```

The command returns an error indicating that the address is unable to lock the object. This is a valid error because the address used for the command, the active address, no longer owns the object.

To operate on this object, use the recipient address, `$RECIPIENT`:

```shell
$ sui client switch --address $RECIPIENT
```

And try the to delete the object again:

```shell
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "delete" --args \"$OBJECT\"
```

In the `Transaction Effects` section of the output, you see a list of deleted objects.

This shows that the object was successfully deleted. If you run the command again:

```shell
$ sui client objects $RECIPIENT
```

You see that the object is no longer listed for the address.