## Chapter 3: Immutable Objects
In chapters 1 and 2, we learned how to create and use objects owned by an account address. In this chapter, we will demonstrate how to create and use immutable objects.

Objects in Sui can have different types of [ownership](../objects.md#object-ownership), with two broad categories: immutable objects and mutable objects. An immutable object is an object that can **never** be mutated, transferred or deleted. Because of this immutability, the object is not owned by anyone, and hence it can be used by anyone.

### Create immutable object

Regardless of whether an object was just created or already owned by an account, to turn this object into an immutable object, we need to call the following API in the [Transfer Library](../../../../sui_programmability/framework/sources/Transfer.move):
```rust
public native fun freeze_object<T: key>(obj: T);
```
After this call, the specified object will become permanently immutable. This is a non-reversible operation; hence, freeze an object only when you are certain that it will never need to be mutated.

Let's add an entry function to the [ColorObject](../../../../sui_programmability/examples/objects_tutorial/sources/ColorObject.move) module to turn an existing (owned) `ColorObject` into an immutable object:
```rust
public(script) fun freeze_object(object: ColorObject, _ctx: &mut TxContext) {
    Transfer::freeze_object(object)
}
```
In the above function, one must already own a `ColorObject` to be able to pass it in. At the end of this call, this object is *frozen* and can never be mutated. It is also no longer owned by anyone.
> :bulb: Note the `Transfer::freeze_object` API requires passing the object by value. Had we allowed passing the object by a mutable reference, we would then still be able to mutate the object after the `freeze_object` call; this contradicts the fact that it should have become immutable.

Alternatively, you can also provide an API that creates an immutable object at birth:
```rust
public(script) fun create_immutable(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
    let color_object = new(red, green, blue, ctx);
    Transfer::freeze_object(color_object)
}
```
In this function, a fresh new `ColorObject` is created and immediately turned into an immutable object before being owned by anyone.

### Use immutable object
Once an object becomes immutable, the rules of who could use this object in Move calls change:
1. An immutable object can be passed only as a read-only reference to Move entry functions as `&T`.
2. Anyone can use immutable objects.

Recall that we defined a function that copies the value of one object to another:
```rust
public(script) fun copy_into(from_object: &ColorObject, into_object: &mut ColorObject, _ctx: &mut TxContext);
```
In this function, anyone can pass an immutable object as the first argument `from_object`, but not the second argument.

Since immutable objects can never be mutated, there will never be a data race even when multiple transactions are using the same immutable object at the same time. Hence, the existence of immutable objects does not pose any requirement on consensus.

### Test immutable object
Let's take a look at how we interact with immutable objects in unit tests.

Previously, we used the `TestScenario::take_owned` API to take an object from the global storage that's owned by the sender of the transaction in a unit test. Since immutable objects are not owned by anyone, `TestScenario::take_owned` works for immutable objects as well! That is, if there exists an immutable object of type `T` in the global storage, `take_owned<T>` will return that object.

Let's see it work in action:
```rust
#[test]
public(script) fun test_immutable() {
    let sender1 = @0x1;
    let scenario = &mut TestScenario::begin(&sender1);
    {
        let ctx = TestScenario::ctx(scenario);
        ColorObject::create_immutable(255, 0, 255, ctx);
    };
    TestScenario::next_tx(scenario, &sender1);
    {
        assert!(TestScenario::can_take_owned<ColorObject>(scenario), 0);
    };
    let sender2 = @0x2;
    TestScenario::next_tx(scenario, &sender2);
    {
        assert!(TestScenario::can_take_owned<ColorObject>(scenario), 0);
    };
}
```
In this test, we submit a transaction as `sender1`, which would create an immutable object. To show that this object can indeed be used by anyone, we start two new transactions, one with `sender1` and another with `sender2`. In both transactions, we are able to take the object.

Next let's examine if this object is indeed immutable. To test this, let's first introduce a function that would mutate a `ColorObject`:
```rust
public(script) fun update(
    object: &mut ColorObject,
    red: u8, green: u8, blue: u8,
    _ctx: &mut TxContext,
) {
    object.red = red;
    object.green = green;
    object.blue = blue;
}
```
Now let's see what happens if we try to call the `update` function on an immutable object:
```rust
#[test]
#[expected_failure(abort_code = 101)]
public(script) fun test_mutate_immutable() {
    let sender1 = @0x1;
    let scenario = &mut TestScenario::begin(&sender1);
    {
        let ctx = TestScenario::ctx(scenario);
        ColorObject::create_immutable(255, 0, 255, ctx);
    };
    TestScenario::next_tx(scenario, &sender1);
    {
        let object = TestScenario::take_owned<ColorObject>(scenario);
        let ctx = TestScenario::ctx(scenario);
        ColorObject::update(&mut object, 0, 0, 0, ctx);
        TestScenario::return_owned(scenario, object);
    };
}
```
Here we defined a test that we expect to fail. `#[expected_failure(abort_code = N)]` is a function attribute that tells Move we expect this test to fail with `abort_code = N`. `101` is the abort code when we try to mutate an immutable object in Move unit tests.

In this test, we first created an immutable object, and latter we try to mutate its value. When we return this object back to the test storage, it will detect that we were mutating an immutable object and abort.

> :bulb: Note that in actual transactions, trying to mutate an immutable object will fail much earlier, even before it has a chance to enter Move VM. We catch this issue when checking the function arguments against the provided objects.

### On-chain interactions
First of all, take a look at the current list of objects you own:
```
$ export ADDR=`wallet active-address`
$ wallet objects --address=$ADDR
```

Let's publish the `ColorObject` code on-chain using the wallet:
```
$ wallet publish --path $ROOT/sui_programmability/examples/objects_tutorial --gas-budget 10000
```
Set the package object ID to the `$PACKAGE` environment variable as we did in previous chapters.

Then create a new `ColorObject`:
```
$ wallet call --gas-budget 1000 --package $PACKAGE --module "ColorObject" --function "create" --args 0 255 0
```
Set the newly created object ID to `$OBJECT`. If we look at the list of objects in the current active account address's wallet:
```
$ wallet objects --address=$ADDR
```
There should be one more, with ID `$OBJECT`. Let's turn it into an immutable object:
```
$ wallet call --gas-budget 1000 --package $PACKAGE --module "ColorObject" --function "freeze_object" --args \"0x$OBJECT\"
```
Now let's look at the list of objects we own again:
```
$ wallet objects --address=$ADDR
```
`$OBJECT` is no longer there. It's no longer owned by anyone. You can see that it's now immutable by querying the object information:
```
$ wallet object --id $OBJECT
Owner: Immutable
...
```
If we try to mutate it:
```
$ wallet call --gas-budget 1000 --package $PACKAGE --module "ColorObject" --function "update" --args \"0x$OBJECT\" 0 0 0
```
It will complain that an immutable object cannot be passed to a mutable argument.
