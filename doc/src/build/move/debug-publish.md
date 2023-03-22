---
title: Debug and Publish the Sui Move Package
---

## Debugging a package
At the moment, there isn't a debugger for Move. To aid with debugging, however, you can use the `std::debug` module to print out arbitrary values. To do so, first import the `debug` module in your source file:
```
use std::debug;
```
Then in places where you want to print out a value `v`, regardless of its type, add the following code:
```
debug::print(&v);
```
or the following if `v` is already a reference:
```
debug::print(v);
```
The `debug` module also provides a function to print out the current stacktrace:
```
debug::print_stack_trace();
```
Alternatively, any call to `abort` or assertion failure also prints the stacktrace at the point of failure.

**Important:** You must remove all calls to functions in the `debug` module from no-test code before you can publish the new module (test code is marked with the `#[test]` annotation).

## Publishing a package

For functions in a Sui Move package to actually be callable from Sui (rather than an emulated Sui execution scenario), you have to publish the package to the Sui [distributed ledger](../../learn/how-sui-works.md), where it is represented as an immutable Sui object.

At this point, however, the `sui move` command does not support package publishing. In fact, it is not clear if it even makes sense to accommodate package publishing, which happens once per package creation, in the context of a unit testing framework. Instead, you can use the Sui CLI client to
[publish](../cli-client.md#publish-packages) Move code and to [call](../cli-client.md#calling-move-code) that code. See the [Sui CLI client documentation](../cli-client.md) for information on how to publish the package created in this tutorial.

### Module initializers

There is, however, an important aspect of publishing packages that affects Move code development in Sui - each module in a package can
include a special _initializer function_ that runs at publication time. The goal of an initializer function is to pre-initialize module-specific data (to create singleton objects). The initializer function must have the following properties for it to execute at publication:

- Function name must be `init`
- A single parameter of `&mut TxContext` type
- No return values
- Private visibility

While the `sui move` command does not support publishing explicitly, you can still test module initializers using the testing framework by dedicating the first transaction to executing the initializer function.

Continuing the fantasy game example, the `init` function should create a `Forge` object. 

``` rust
    // Module initializer to be executed when this module is published
    fun init(ctx: &mut TxContext) {
        let admin = Forge {
            id: object::new(ctx),
            swords_created: 0,
        };
        // Transfer the Forge object to the module/package publisher
        // (presumably the game admin)
        transfer::transfer(admin, tx_context::sender(ctx));
    }
```

The tests you have so far call the `init` function, but the initializer function itself isn't tested to ensure it properly creates a `Forge` object. To test this functionality, modify the `sword_create` function to take the forge as a parameter and to update the number of
created swords at the end of the function:

``` rust
    public entry fun sword_create(forge: &mut Forge, magic: u64, strength: u64, recipient: address, ctx: &mut TxContext) {
        ...
        forge.swords_created = forge.swords_created + 1;
    }
```

Now, create a function to test the module initialization:

``` rust
    #[test]
    public fun test_module_init() {
        use sui::test_scenario;

        // Create test address representing game admin
        let admin = @0xBABE;

        // First transaction to emulate module initialization
        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;
        {
            init(test_scenario::ctx(scenario));
        };
        // Second transaction to check if the forge has been created
        // and has initial value of zero swords created
        test_scenario::next_tx(scenario, admin);
        {
            // Extract the Forge object
            let forge = test_scenario::take_from_sender<Forge>(scenario);
            // Verify number of created swords
            assert!(swords_created(&forge) == 0, 1);
            // Return the Forge object to the object pool
            test_scenario::return_to_sender(scenario, forge);
        };
        test_scenario::end(scenario_val);
    }

```

As the new test function shows, the first transaction (explicitly) calls the initializer. The next transaction checks if the `Forge` object has been created and properly initialized.

If you try to run tests on the whole package at this point, you encounter compilation errors in the existing tests because of the
`sword_create` function signature change. The changes required for the tests to run again is an exercise left for you. If you need help, you can refer to the source code for the package (with all the tests properly adjusted) in [my_module.move](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/move_tutorial/sources/my_module.move).