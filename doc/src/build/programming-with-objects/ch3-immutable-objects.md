---
title: Chapter 3 - Immutable Objects
---

Chapters 1 and 2 describe how to create and use objects owned by an address. This chapter describes how to create and use immutable objects.

Objects in Sui can have different types of [ownership](../objects.md#object-ownership), with two broad categories: immutable objects and mutable objects. An immutable object is an object that can't be mutated, transferred or deleted. Immutable objects have no owner, so anyone can use them.

### Create immutable object

To convert an object into an immutable object, call the following function in the [transfer module](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/transfer.move):

```rust
public native fun freeze_object<T: key>(obj: T);
```

This call makes the specified object immutable. This is a non-reversible operation. You should freeze an object only when you are certain that you don't need to mutate it.

Add an entry function to the [color_object module](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/objects_tutorial/sources/color_object.move) to turn an existing (owned) `ColorObject` into an immutable object:

```rust
public entry fun freeze_object(object: ColorObject) {
    transfer::freeze_object(object)
}
```

In the preceding function, you must already own a `ColorObject` to pass it in. At the end of this call, this object is *frozen* and can never be mutated. It is also no longer owned by anyone.

Note that the `transfer::freeze_object` function requires you to pass the object by value. If the object allowed passing the object by a mutable reference, you could still mutate the object after the `freeze_object` call. This contradicts the fact that it should have become immutable.

Alternatively, you can also provide an API that creates an immutable object at creation:

```rust
public entry fun create_immutable(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
    let color_object = new(red, green, blue, ctx);
    transfer::freeze_object(color_object)
}
```

This function creates a new `ColorObject` and immediately makes it immutable before it has an owner.

### Use immutable object

Once an object becomes immutable, the rules of who can use this object in Sui Move calls change:
1. An immutable object can be passed only as a read-only, immutable reference to Sui Move entry functions as `&T`.
2. Anyone can use immutable objects.

In a preceding section, you defined a function that copies the value of one object to another:

```rust
public entry fun copy_into(from_object: &ColorObject, into_object: &mut ColorObject);
```

In this function, anyone can pass an immutable object as the first argument `from_object`, but not the second argument.

Since immutable objects can never be mutated, there's no data race, even when multiple transactions are using the same immutable object at the same time. Hence, the existence of immutable objects does not pose any requirement on consensus.

### Test immutable object

Previously, you used the `test_scenario::take_from_sender<T>` function to take an object from the global storage that's owned by the sender of the transaction in a unit test. And `take_from_sender` returns an object by value, which allows you to mutate, delete, or transfer it.

To take an immutable object, use a new function: `test_scenario::take_immutable<T>`. This is required because you can access immutable objects only through read-only references. The `test_scenario` runtime keeps track of the usage of this immutable object. If the compiler does not return the object via `test_scenario::return_immutable` before the start of the next transaction, the test stops.

To see it work in action, (`ColorObjectTests::test_immutable`):

```rust
let sender1 = @0x1;
let scenario_val = test_scenario::begin(sender1);
let scenario = &mut scenario_val;
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create_immutable(255, 0, 255, ctx);
};
test_scenario::next_tx(scenario, sender1);
{
    // take_owned does not work for immutable objects.
    assert!(!test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
```

This test submits a transaction as `sender1`, which tries to create an immutable object.

The `has_most_recent_for_sender<ColorObject>` function no longer returns `true`, because the object is no longer owned. To take this object:

```rust
// Any sender can work.
let sender2 = @0x2;
test_scenario::next_tx(scenario, sender2);
{
    let object = test_scenario::take_immutable<ColorObject>(scenario);
    let (red, green, blue) = color_object::get_color(&object);
    assert!(red == 255 && green == 0 && blue == 255, 0);
    test_scenario::return_immutable(object);
};

test_scenario::end(scenario_val);
```

To show that this object is indeed not owned by anyone, start the next transaction with `sender2`. Note that it used `take_immutable` and succeeded. This means that any sender can take an immutable object. To return the object, call a new function: `return_immutable`.

To examine whether this object is immutable, add a function that tries to mutate a `ColorObject`:

```rust
public entry fun update(
    object: &mut ColorObject,
    red: u8, green: u8, blue: u8,
) {
    object.red = red;
    object.green = green;
    object.blue = blue;
}
```

This section introduced two new functions to interact with immutable objects in unit tests:
- `test_scenario::take_immutable<T>` to take an immutable object wrapper from global storage.
- `test_scenario::return_immutable` to return the wrapper back to the global storage.


### On-chain interactions

First, view the objects you own:

```shell
$ export ADDR=`sui client active-address`
$ sui client objects $ADDR
```

Publish the `ColorObject` code on-chain using the Sui Client CLI:

```shell
sui client publish $ROOT/sui_programmability/examples/objects_tutorial --gas-budget 10000
```

Set the package object ID to the `$PACKAGE` environment variable as described in previous chapters. Then create a new `ColorObject`:

```shell
sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "create" --args 0 255 0
```

Set the newly created object ID to `$OBJECT`. To view the objects in the current active address:

```shell
$ sui client objects $ADDR
```

You should see an object with the ID you used for `$OBJECT`. To turn it into an immutable object:

```shell
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "freeze_object" --args \"$OBJECT\"
```

View the list of objects again:

```shell
$ sui client objects $ADDR
```

`$OBJECT` is no longer listed. It's no longer owned by anyone. You can see that it's now immutable by querying the object information:

```shell
$ sui client object $OBJECT
```

The response includes:

```
Owner: Immutable
```

If you try to mutate it:

```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "update" --args \"$OBJECT\" 0 0 0
```

The response indicates that you can't pass an immutable object to a mutable argument.
