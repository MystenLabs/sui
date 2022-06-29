---
title: Chapter 1 - Object Basics
---

### Define Sui Object
In Move, besides primitive data types, we can define organized data structures using `struct`. For example:
```rust
struct Color {
    red: u8,
    green: u8,
    blue: u8,
}
```
The above `struct` defines a data structure that can represent RGB color. `struct`s like this can be used to organize data with complicated semantics. However, instances of `struct`s like `Color` are not Sui objects yet.
To define a struct that represents a Sui object type, we must add a `key` capability to the definition, and the first field of the struct must be the `id` of the object with type `VersionedID` from the [ID library](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/id.move):
```rust
use sui::id::VersionedID;

struct ColorObject has key {
    id: VersionedID,
    red: u8,
    green: u8,
    blue: u8,
}
```
Now `ColorObject` represents a Sui object type and can be used to create Sui objects that can be eventually stored on the Sui chain.
> :books: In both core Move and Sui Move, the [key ability](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#key) denotes a type that can appear as a key in global storage. However, the structure of global storage is a bit different: core Move uses a (type, `address`)-indexed map, whereas Sui Move uses a map keyed by object IDs.

> :bulb: The `VersionedID` type is internal to Sui, and you most likely won't need to deal with it directly. For curious readers, it contains the unique `ID` of the object and the version of the object. Each time a mutable object is used in a transaction, its version will increase by 1.

### Create Sui object
Now that we have learned how to define a Sui object type, how do we create/instantiate a Sui object? In order to create a new Sui object from its type, we must assign an initial value to each of the fields, including `id`. The only way to create a new unique `VersionedID` for a Sui object is to call `tx_context::new_id`. The `new_id` function takes the current transaction context as an argument to generate unique IDs. The transaction context is of type `&mut TxContext` and should be passed down from an [entry function](../move.md#entry-functions) (a function that can be called directly from a transaction). Let's look at how we may define a constructor for `ColorObject`:
```rust
/// tx_context::Self represents the TxContext module, which allows us call
/// functions in the module, such as the `new_id` function.
/// tx_context::TxContext represents the TxContext struct in TxContext module.
use sui::tx_context::{Self, TxContext};

fun new(red: u8, green: u8, blue: u8, ctx: &mut TxContext): ColorObject {
    ColorObject {
        id: tx_context::new_id(ctx),
        red,
        green,
        blue,
    }
}
```
> :bulb: Move supports *field punning*, which allows us to skip the field values if the field name happens to be the same as the name of the value variable it is bound to. The code above leverages this to write "`red,`" as shorthand for "`red: red,`".

### Store Sui object
We have defined a constructor for the `ColorObject`. Calling this constructor will put the value in a local variable where it can be returned from the current function, passed to other functions, or stored inside another struct. And of course, the object can be placed in persistent global storage so it can be read by the outside world and accessed in subsequent transactions.

All of the APIs for adding objects to persistent storage live in the [`Transfer`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move) module. One key API is:
```rust
public fun transfer<T: key>(obj: T, recipient: address)
```
This places `obj` in global storage along with metadata that records `recipient` as the owner of the object. In Sui, every object must have an owner, which can be either an account address, another object, or "shared"--see [object ownership](../objects.md#object-ownership) for more details.

> :bulb: In core Move, we would call `move_to<T>(a: address, t: T)` to add the entry `(a, T) -> t` to the global storage. But because (as explained above) the schema of Sui Move's global storage is different, we use the `Transfer` APIs instead of `move_to` or the other [global storage operators](https://github.com/move-language/move/blob/main/language/documentation/book/src/global-storage-operators.md) in core Move. These operators cannot be used in Sui Move.

A common use of this API is to transfer the object to the sender/signer of the current transaction (e.g., mint an NFT owned by you). The only way to obtain the sender of the current transaction is to rely on the transaction context passed in from an entry function. The last argument to an entry function must be the current transaction context, defined as `ctx: &mut TxContext`.
To obtain the current signer's address, one can call `tx_context::sender(ctx)`.

Below is the code that creates a new `ColorObject` and makes it owned by the sender of the transaction:
```rust
use sui::transfer;

// This is an entry function that can be called directly by a Transaction.
public entry fun create(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
    let color_object = new(red, green, blue, ctx);
    transfer::transfer(color_object, tx_context::sender(ctx))
}
```
> :bulb: Naming convention: Constructors are typically named **`new`**, which returns an instance of the struct type. The **`create`** function is typically defined as an entry function that constructs the struct and transfers it to the desired owner (most commonly the sender).

We can also add a getter to `ColorObject` that returns the color values so that modules outside of `ColorObject` are able to read their values:
```rust
public fun get_color(self: &ColorObject): (u8, u8, u8) {
    (self.red, self.green, self.blue)
}
```

Find the full code online in [color_object.move](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/objects_tutorial/sources/color_object.move).

To compile the code, make sure you have [installed Sui](../install.md) so that `sui-move` is in `PATH`. In the code root directory (where `Move.toml` is), run:
```
sui-move build
```

### Writing unit tests
After defining the `create` function, we want to test this function in Move using unit tests, without having to go all the way through sending Sui transactions. Since [Sui manages global storage separately outside of Move](../../learn/sui-move-diffs.md#object-centric-global-storage), there is no direct way to retrieve objects from global storage within Move. This poses a question: after calling the `create` function, how do we check that the object is properly transferred?

To assist easy testing in Move, we provide a comprehensive testing framework in the [test_scenario](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/test_scenario.move) module that allows us to interact with objects put into the global storage. This allows us to test the behavior of any function directly in Move unit tests. A lot of this is also covered in our [Move testing doc](../move.md#sui-specific-testing).

The idea of `test_scenario` is to emulate a series of Sui transactions, each sent from a particular address. A developer writing a test starts the first transaction using the `test_scenario::begin` function that takes the address of the user sending this transaction as an argument and returns an instance of the `Scenario` struct representing a test scenario.

An instance of the `Scenario` struct contains a per-address object pool emulating Sui's object storage, with helper functions provided to manipulate objects in the pool. Once the first transaction is finished, subsequent transactions can be started using the `test_scenario::next_tx` function that takes an instance of the `Scenario` struct representing the current scenario and an address of a (new) user as arguments.

Now let's try to write a test for the `create` function. Tests that need to use `test_scenario` must be in a separate module, either under a `tests` directory, or in the same file but in a module annotated with `#[test_only]`. This is because `test_scenario` itself is a test-only module, and can be used only by test-only modules.

First of all, we begin the test with a hardcoded test address, which will also give us a transaction context as if we are sending a transaction from this address. We then call the `create` function, which should create a `ColorObject` and transfer it to the test address:
```rust
let owner = @0x1;
// Create a ColorObject and transfer it to @owner.
let scenario = &mut test_scenario::begin(&owner);
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 0, 255, ctx);
};
```
>:books: Note there is a "`;`" after "`}`". This is required except for the last statement in the function. Refer to the [Move book](https://move-book.com/syntax-basics/expression-and-scope.html) for a detailed explanation.

Now account `@0x1` should own the object. Let's first make sure it's not owned by anyone else:
```rust
let not_owner = @0x2;
// Check that @not_owner does not own the just-created ColorObject.
test_scenario::next_tx(scenario, &not_owner);
{
    assert!(!test_scenario::can_take_owned<ColorObject>(scenario), 0);
};
```

`test_scenario::next_tx` switches the transaction sender to `@0x2`, which is a new address than the previous one.
`test_scenario::can_take_owned` checks whether an object with the given type actually exists in the global storage owned by the current sender of the transaction. In this code, we assert that we should not be able to remove such an object, because `@0x2` does not own any object.
> :bulb: The second parameter of `assert!` is the error code. In non-test code, we usually define a list of dedicated error code constants for each type of error that could happen in production. For unit tests though, it's usually unnecessary because there will be way too many assetions and the stacktrace upon error is sufficient to tell where the error happened. Hence we recommend just putting `0` there in unit tests for assertions.

Finally we check that `@0x1` owns the object and the object value is consistent:
```rust
test_scenario::next_tx(scenario, &owner);
{
    let object = test_scenario::take_owned<ColorObject>(scenario);
    let (red, green, blue) = color_object::get_color(&object);
    assert!(red == 255 && green == 0 && blue == 255, 0);
    test_scenario::return_owned(scenario, object);
};
```

`test_scenario::take_owned` removes the object of given type from global storage that's owned by the current transaction sender (it also implicitly checks `can_take_owned`). If this line of code succeeds, it means that `owner` indeed owns an object of type `ColorObject`.
We also check that the field values of the object match with what we set in creation. At the end, we must return the object back to the global storage by calling `test_scenario::return_owned` so that it's back to the global storage. This also ensures that if any mutations happened to the object during the test, the global storage is aware of the changes.

Again, you can find the full code in [color_object.move](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/objects_tutorial/sources/color_object.move).

To run the test, simply run the following in the code root directory:
```
sui-move test
```

### On-chain Interactions
Now let's try to call `create` in actual transactions and see what happens. To do this, we need to start Sui and the CLI client. Follow the [Sui CLI client guide](../cli-client.md) to start the Sui network and set up the client.

Before starting, let's take a look at the default client address (this is the address that will eventually own the object later):
```
$ sui client active-address
```
This will tell you the current client address.

First, we need to publish the code on-chain. Assuming the path to the root of the repository containing Sui source code is $ROOT:
```
$ sui client publish --path $ROOT/sui_programmability/examples/objects_tutorial --gas-budget 10000
```
You can find the published package object ID in the **Publish Results** output:
```
----- Publish Results ----
The newly published package object: (0x57258f32746fd1443f2a077c0c6ec03282087c19, SequenceNumber(1), o#b3a8e284dea7482891768e166e4cd16f9749e0fa90eeb0834189016c42327401)
```
Note that the exact data you see will be different. The first hex string in that triple is the package object ID (`0x57258f32746fd1443f2a077c0c6ec03282087c19` in this case). For convenience, let's save it to an environment variable:
```
$ export PACKAGE=0x57258f32746fd1443f2a077c0c6ec03282087c19
```
Next we can call the function to create a color object:
```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "ColorObject" --function "create" --args 0 255 0
```
In the **Transaction Effects** portion of the output, you will see an object showing up in the list of **Created Objects**, like this:

```
Created Objects:
0x5eb2c3e55693282faa7f5b07ce1c4803e6fdc1bb SequenceNumber(1) o#691b417670979c6c192bdfd643630a125961c71c841a6c7d973cf9429c792efa
```
Again, for convenience, let's save the object ID:
```
$ export OBJECT=0x5eb2c3e55693282faa7f5b07ce1c4803e6fdc1bb
```
We can inspect this object and see what kind of object it is:
```
$ sui client object --id $OBJECT
```
This will show you the metadata of the object with its type:
```
Owner: AddressOwner(k#5db53ebb05fd3ea5f1d163d9d487ee8cd7b591ee)
Version: 1
ID: 0x5eb2c3e55693282faa7f5b07ce1c4803e6fdc1bb
Readonly: false
Type: 0x57258f32746fd1443f2a077c0c6ec03282087c19::color_object::ColorObject
```
As we can see, it's owned by the current default client address that we saw earlier. And the type of this object is `ColorObject`!

You can also look at the data content of the object by adding the `--json` parameter:
```
$ sui client object --id $OBJECT --json
```
This will print the values of all the fields in the Move object, such as the values of `red`, `green`, and `blue`.

Congratulations! You have learned how to define, create, and transfer objects. You should also know how to write unit tests to mock transactions and interact with the objects. In the next chapter, we will learn how to use the objects that we own.
