---
title: Chapter 1 - Object Basics
---

### Define Sui Object

In Sui Move, besides primitive data types, you can define organized data structures using `struct`. For example:
```rust
struct Color {
    red: u8,
    green: u8,
    blue: u8,
}
```

The `struct` defines a data structure to represent RGB color. You can use a `struct` like this to organize data with complicated semantics. However, an instance of a `struct`, such as `Color`, is not a Sui object yet.
To define a struct that represents a Sui object type, you must add a `key` capability to the definition. The first field of the struct must be the `id` of the object with type `UID` from the [object module](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/object.move) - a module from the core [Sui Framework](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/).

```rust
use sui::object::UID;

struct ColorObject has key {
    id: UID,
    red: u8,
    green: u8,
    blue: u8,
}
```

The `ColorObject` represents a Sui object type that you can use to create Sui objects that can eventually be stored on the Sui network.

**Important:** In both core Move and Sui Move, the [key ability](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#key) denotes a type that can appear as a key in global storage. However, the structure of global storage is a bit different: core Move uses a (type, `address`)-indexed map, whereas Sui Move uses a map keyed by object IDs.

The `UID` type is internal to Sui, and you most likely won't need to deal with it directly. For curious readers, it contains the "unique ID" that defines an object on the Sui network. It is unique in the sense that no two values of type `UID` will ever have the same underlying set of bytes.

### Create Sui object

After you define a Sui object type you can create or instantiate a Sui object. To create a new Sui object from its type, you must assign an initial value to each of the fields, including `id`. The only way to create a new `UID` for a Sui object is to call `object::new`. The `new` function takes the current transaction context as an argument to generate unique IDs. The transaction context is of type `&mut TxContext` and should be passed down from an [entry function](../move/index.md#entry-functions). You can call `Entry` functions directly from a transaction. 

To define a constructor for `ColorObject`:

```rust
// object creates an alias to the object module, which allows you to call
// functions in the module, such as the `new` function, without fully
// qualifying, for example `sui::object::new`.
use sui::object;
// tx_context::TxContext creates an alias to the TxContext struct in the tx_context module.
use sui::tx_context::TxContext;


fun new(red: u8, green: u8, blue: u8, ctx: &mut TxContext): ColorObject {
    ColorObject {
        id: object::new(ctx),
        red,
        green,
        blue,
    }
}
```

Sui Move supports *field punning*, which allows you to skip the field values if the field name happens to be the same as the name of the value variable it is bound to. The preceding code example leverages this to write "`red,`" as shorthand for "`red: red,`".

### Store Sui object

You now have a constructor for the `ColorObject`. If you call this constructor, it puts the value in a local variable. The local variable can be returned from the current function, passed to other functions, or stored inside another struct. The object can be placed in persistent global storage, be read by anyone, and accessed in subsequent transactions.

All of the APIs for adding objects to persistent storage are defined in the [`transfer`](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/transfer.move) module. One key API is:

```rust
public fun transfer<T: key>(obj: T, recipient: address)
```

This places `obj` in global storage along with the metadata that records `recipient` as the owner of the object. In Sui, every object must have an owner. The owner can be either an address, another object, or "shared". See [Object ownership](../../learn/objects.md#object-ownership) for more details.

In core Move, you call `move_to<T>(a: address, t: T)` to add the entry `(a, T) -> t` to the global storage. But the schema of Sui Move's global storage is different, so you can use the `Transfer` APIs instead of `move_to` or the other [global storage operators](https://github.com/move-language/move/blob/main/language/documentation/book/src/global-storage-operators.md) in core Move. You can't use these operators in Sui Move.

A common use of this API is to transfer the object to the sender/signer of the current transaction, such as when you mint an NFT owned by you. The only way to obtain the sender of the current transaction is to rely on the transaction context passed in from an `entry` function. The last argument to an `entry` function must be the current transaction context, defined as `ctx: &mut TxContext`.

To obtain the current signer's address, you can call `tx_context::sender(ctx)`.

The following code sample creates a new `ColorObject` and sets the owner to the sender of the transaction:

```rust
use sui::transfer;

// This is an entry function that you can call directly by a Transaction.
public entry fun create(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
    let color_object = new(red, green, blue, ctx);
    transfer::transfer(color_object, tx_context::sender(ctx))
}
```

**Note:** Naming convention: Constructors are typically named `new`, which returns an instance of the struct type. The `create` function is typically defined as an entry function that constructs the struct and transfers it to the desired owner (most commonly the sender).

You can also add a getter to `ColorObject` that returns the color values so that modules outside of `ColorObject` are able to read their values:

```rust
public fun get_color(self: &ColorObject): (u8, u8, u8) {
    (self.red, self.green, self.blue)
}
```

Find the full code in the Sui repo under `sui_programmability/examples/objects_tutorial/sources/` in [color_object.move](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/objects_tutorial/sources/color_object.move).

To compile the code, make sure you have [installed Sui](../install.md) so that `sui` is in your `PATH`. In the code root directory `(../examples/objects_tutorial/)` (where `Move.toml` is), run:

```
sui move build
```

### Writing unit tests

After you define the `create` function, you can test it in Sui Move using unit tests without having to go all the way through sending Sui transactions. Since [Sui manages global storage separately outside of Move](../../learn/sui-move-diffs.md#object-centric-global-storage), there is no direct way to retrieve objects from global storage within Move. This poses a question: after calling the `create` function, how do you check that the object is properly transferred?

To assist easy testing in Sui Move, Sui provides a comprehensive testing framework in the [test_scenario](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/test/test_scenario.move) module that allows us to interact with objects put into the global storage. This allows us to test the behavior of any function directly in Sui Move unit tests. A lot of this is also covered in our [Move testing](../move/build-test.md#sui-specific-testing) topic.

The `test_scenario` emulates a series of Sui transactions, each sent from a particular address. You can start the first transaction using the `test_scenario::begin` function that takes the address of the user sending this transaction as an argument, and returns an instance of the `Scenario` struct representing a test scenario.

An instance of the `Scenario` struct contains a per-address object pool emulating Sui's object storage, with helper functions provided to manipulate objects in the pool. After the first transaction completes, you can start subsequent transactions using the `test_scenario::next_tx` function that takes an instance of the `Scenario` struct representing the current scenario and an address of a (new) user as arguments.

Next, write a test for the `create` function. Tests that need to use `test_scenario` must be in a separate module, either under a `tests` directory, or in the same file but in a module annotated with `#[test_only]`. This is because `test_scenario` itself is a test-only module, and can be used only by test-only modules.

Start the test with a hardcoded test address, which gives you a transaction context as if you sent the transaction that starts with `test_scenario::begin` from this address. You can then call the `create` function, which creates a `ColorObject` and transfers it to the test address:

```rust
let owner = @0x1;
// Create a ColorObject and transfer it to @owner.
let scenario_val = test_scenario::begin(owner);
let scenario = &mut scenario_val;
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 0, 255, ctx);
};
```

**Note:** There is a "`;`" after "`}`". You must include `;` to sequence a series of expressions, and even the block `{ ... }` is an expression. Refer to the [Move book](https://move-book.com/syntax-basics/expression-and-scope.html) for a detailed explanation.

After the first transaction completes (**and only after the first transaction completes**), address `@0x1` owns the object. First, make sure it's not owned by anyone else:

```rust
let not_owner = @0x2;
// Check that not_owner does not own the just-created ColorObject.
test_scenario::next_tx(scenario, not_owner);
{
    assert!(!test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
```

`test_scenario::next_tx` switches the transaction sender to `@0x2`, which is a new address different from the previous one.
`test_scenario::has_most_recent_for_sender` checks whether an object with the given type actually exists in the global storage owned by the current sender of the transaction. This code asserts that you should not be able to remove such an object, because `@0x2` does not own any object.

**Note:** The second parameter of `assert!` is the error code. In non-test code, you usually define a list of dedicated error code constants for each type of error that could happen in production. For unit tests, it's usually unnecessary because there are too many assertions. The stack trace upon error is sufficient to tell where the error happened. You can just put `0` for assertions in unit tests.

Finally, check that `@0x1` owns the object and the object value is consistent:

```rust
test_scenario::next_tx(scenario, owner);
{
    let object = test_scenario::take_from_sender<ColorObject>(scenario);
    let (red, green, blue) = color_object::get_color(&object);
    assert!(red == 255 && green == 0 && blue == 255, 0);
    test_scenario::return_to_sender(scenario, object);
};
test_scenario::end(scenario_val);
```

`test_scenario::take_from_sender` removes the object of given type from global storage that's owned by the current transaction sender (it also implicitly checks `has_most_recent_for_sender`). If this line of code succeeds, it means that `owner` indeed owns an object of type `ColorObject`.
Also check that the field values of the object match with what you set in creation. You must return the object back to the global storage by calling `test_scenario::return_to_sender` so that it's back to the global storage. This also ensures that if any mutations happened to the object during the test, the global storage is aware of the changes.

You can find the full code in [color_object.move](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/objects_tutorial/sources/color_object.move).

To run the test, run the following in the code root directory:

```
sui move test
```

### On-chain Interactions

To call `create` in actual transactions, you need to start Sui and the Sui Client CLI. Follow the [Sui CLI client guide](../cli-client.md) to start the Sui network and set up the client.

Before you start, check the active address on the client as that address eventually owns the object):

```
$ sui client active-address
```

To publish the code on-chain, use the following command:

```
$ sui client publish $ROOT/sui_programmability/examples/objects_tutorial --gas-budget 10000
```

or from the root of the package folder:

```
$ sui client publish --gas-budget 10000
```

These examples assume that the path to the root of the repository containing Sui source code is $ROOT.


You can find the published package object ID in the **Transaction Effects** output:

```
Transaction Kind : Publish
----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0x225019dc52210704642b76c0bcf0d05bd374b6a348080f82a30ce7f8303c1b3f , Owner: Immutable
Mutated Objects:
  - ID: 0x1b879f00b03357c95a908b7fb568712f5be862c5cb0a5894f62d06e9098de6dc , Owner: Account Address ( 0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446 )
```

Note that the exact data you see differs from the examples in this topic.

The first hex string with the `Immutable` owner is the package's `objectID`  (`0x79b81364676f2f700e8a5acc71ca66eef753f1e536e4480a24278f02499e8cc5`). For convenience, save it to an environment variable:

```
export PACKAGE=0x79b81364676f2f700e8a5acc71ca66eef753f1e536e4480a24278f02499e8cc5
```
The mutated object is the gas object used to pay for the transaction.

You can call the function to create a color object:

```shell
sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "create" --args 0 255 0
```

In the **Transaction Effects** portion of the output, you see an object included in the list of **Created Objects**:

```
----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446 , Owner: Account Address ( 0x79b81364676f2f700e8a5acc71ca66eef753f1e536e4480a24278f02499e8cc5 )
Mutated Objects:
  - ID: 0x7cd011b6dbe90a0520a8501d993e3666b9373456b588f97600fcae6e02f60aa3 , Owner: Account Address ( 0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446 )
```

To save the object ID as a variable, use:

```
export OBJECT=0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446
```

To inspect this object and see what kind of object it is, use:

```shell
sui client object $OBJECT
```

This returns the metadata of the object, including its type:

```
----- Move Object (0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446[8]) -----
Owner: Account Address ( 0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446 )
Version: 8
Storage Rebate: 14
Previous Transaction: HRrB6qFxQZt7VEzagEjE4nhF9rbffK2wZRxqn9pPLhMk
----- Data -----
type: 0x79b81364676f2f700e8a5acc71ca66eef753f1e536e4480a24278f02499e8cc5::color_object::ColorObject
blue: 0
green: 255
id: 0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446
red: 0
```

You can also request the content of the object in json format by adding the `--json` parameter:
```
$ sui client object $OBJECT --json
```

To continue learning about programming with objects in Sui, see [Using Objects](ch2-using-objects.md).
