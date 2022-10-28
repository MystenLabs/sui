---
title: Build and Test the Sui Move Package
---

## Building a package

Ensure you are in the `my_move_package` directory that contains your package, and then use the following command to build it:

``` shell
$ sui move build
```

A successful build returns a response similar to the following:

```shell
Build Successful
Artifacts path: "./build"
```

If the build fails, you can use the verbose error messaging in output to troubleshoot and resolve root issues.

Now that we have designed our asset and its accessor functions, let us
test the code we have written.

## Testing a package

Sui includes support for the
[Move testing framework](https://github.com/move-language/move/blob/main/language/documentation/book/src/unit-testing.md)
that allows you to write unit tests to test Move code much like test
frameworks for other languages (e.g., the built-in
[Rust testing framework](https://doc.rust-lang.org/rust-by-example/testing/unit_testing.html)
or the [JUnit framework](https://junit.org/) for Java).

An individual Move unit test is encapsulated in a public function that
has no parameters, no return values, and has the `#[test]`
annotation. Such functions are executed by the testing framework
upon executing the following command (in the `my_move_package`
directory as per our running example):

``` shell
$ sui move test
```

If you execute this command for the package created in
[write a package](write-package.md), you
will see the following output indicating, unsurprisingly,
that no tests have ran because we have not written any yet!

``` shell
BUILDING MoveStdlib
BUILDING Sui
BUILDING MyFirstPackage
Running Move unit tests
Test result: OK. Total tests: 0; passed: 0; failed: 0
```

Let us write a simple test function and insert it into the `my_module.move`
file:

``` rust
    #[test]
    public fun test_sword_create() {
        use sui::tx_context;

        // create a dummy TxContext for testing
        let ctx = tx_context::dummy();

        // create a sword
        let sword = Sword {
            id: object::new(&mut ctx),
            magic: 42,
            strength: 7,
        };

        // check if accessor functions return correct values
        assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
    }
```

The code of the unit test function is largely self-explanatory - we
create a dummy instance of the `TxContext` struct needed to create
a unique identifier of our sword object, then create the sword itself,
and finally call its accessor functions to verify that they return
correct values. Note the dummy context is passed to the
`object::new` function as a mutable reference argument (`&mut`),
and the sword itself is passed to its accessor functions as a
read-only reference argument.

Now that we have written a test, let's try to run the tests again:

``` shell
$ sui move test
```

After running the test command, however, instead of a test result we
get a compilation error:

``` shell
error[E06001]: unused value without 'drop'
   ┌─ ./sources/my_module.move:60:65
   │
 4 │       struct Sword has key, store {
   │              ----- To satisfy the constraint, the 'drop' ability would need to be added here
   ·
27 │           let sword = Sword {
   │               ----- The local variable 'sword' still contains a value. The value does not have the 'drop' ability and must be consumed before the function returns
   │ ╭─────────────────────'
28 │ │             id: object::new(&mut ctx),
29 │ │             magic: 42,
30 │ │             strength: 7,
31 │ │         };
   │ ╰─────────' The type 'MyFirstPackage::my_module::Sword' does not have the ability 'drop'
   · │
34 │           assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
   │                                                                   ^ Invalid return
```

This error message looks quite complicated, but it contains all the
information needed to understand what went wrong. What happened here
is that while writing the test, we accidentally stumbled upon one of
the Move language's safety features.

Remember the `Sword` struct represents a game asset
digitally mimicking a real-world item. At the same time, while a sword
in a real world cannot simply disappear (though it can be explicitly
destroyed), there is no such restriction on a digital one. In fact,
this is exactly what's happening in our test function - we create an
instance of a `Sword` struct that simply disappears at the end of the
function call. And this is the gist of the error message we are
seeing.

One of the solutions (as suggested in the message itself),
is to add the `drop` ability to the definition of the `Sword` struct,
which would allow instances of this struct to disappear (be
*dropped*). Arguably, being able to *drop* a valuable asset is not an
asset property we would like to have, so another solution to our
problem is to transfer ownership of the sword.

In order to get our test to work, we then add the following line to
the beginning of our testing function to import the
[Transfer module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move):

``` rust
        use sui::transfer;

```

We then use the `Transfer` module to transfer ownership of the sword
to a freshly created dummy address by adding the following lines to
the end of our test function:

``` rust
        // create a dummy address and transfer the sword
        let dummy_address = @0xCAFE;
        transfer::transfer(sword, dummy_address);
```

We can now run the test command again and see that indeed a single
successful test has been run:

``` shell
BUILDING MoveStdlib
BUILDING Sui
BUILDING MyFirstPackage
Running Move unit tests
[ PASS    ] 0x0::my_module::test_sword_create
Test result: OK. Total tests: 1; passed: 1; failed: 0
```

---
**Tip:**
If you want to run only a subset of the unit tests, you can filter by test name using the `--filter` option. Example:
```
$ sui move test --filter sword
```
The above command will run all tests whose name contains "sword".
You can discover more testing options through:
```
$ sui move test -h
```

---

### Sui-specific testing

The testing example we have seen so far is largely *pure Move* and has
little to do with Sui beyond using some Sui packages, such as
`sui::tx_context` and `sui::transfer`. While this style of testing is
already very useful for developers writing Move code for Sui, they may
also want to test additional Sui-specific features. In particular, a
Move call in Sui is encapsulated in a Sui
[transaction](../transactions.md),
and a developer may wish to test interactions between different
transactions within a single test (e.g. one transaction creating an
object and the other one transferring it).

Sui-specific testing is supported via the
[test_scenario module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/test_scenario.move)
that provides Sui-related testing functionality otherwise unavailable
in *pure Move* and its
[testing framework](https://github.com/move-language/move/blob/main/language/documentation/book/src/unit-testing.md).

The main concept in the `test_scenario` is a scenario that emulates a
series of Sui transactions, each executed by a (potentially) different
user. At a high level, a developer writing a test starts the first
transaction using the `test_scenario::begin` function that takes an
address of the user executing this transaction as the first and only
argument and returns an instance of the `Scenario` struct representing
a scenario.

An instance of the `Scenario` struct contains a
per-address object pool emulating Sui's object storage, with helper
functions provided to manipulate objects in the pool. Once the first
transaction is finished, subsequent transactions can be started using
the `test_scenario::next_tx` function that takes an instance of the
`Scenario` struct representing the current scenario and an address of
a (new) user as arguments.

Let us extend our running example with a multi-transaction test that
uses the `test_scenario` to test sword creation and transfer from the
point of view of a Sui developer. First, let us create
[entry functions](index.md#entry-functions) callable from Sui that implement
sword creation and transfer and put them into the `my_module.move` file:

``` rust
    public entry fun sword_create(magic: u64, strength: u64, recipient: address, ctx: &mut TxContext) {
        use sui::transfer;

        // create a sword
        let sword = Sword {
            id: object::new(ctx),
            magic: magic,
            strength: strength,
        };
        // transfer the sword
        transfer::transfer(sword, recipient);
    }

    public entry fun sword_transfer(sword: Sword, recipient: address, _ctx: &mut TxContext) {
        use sui::transfer;
        // transfer the sword
        transfer::transfer(sword, recipient);
    }
```

The code of the new functions is self-explanatory and uses struct
creation and Sui-internal modules (`TxContext` and `Transfer`) in a
way similar to what we have seen in the previous sections. The
important part is for the entry functions to have correct signatures
as described [earlier](index.md#entry-functions). In order for this code to
build, we need to add an additional import line at the module level
(as the first line in the module's main code block right before the
existing module-wide `ID` module import) to make the `TxContext`
struct available for function definitions:

``` rust
    use sui::tx_context::TxContext;
```

We can now build the module extended with the new functions but still
have only one test defined. Let us change that by adding another test
function.

``` rust
    #[test]
    fun test_sword_transactions() {
        use sui::test_scenario;

        // create test addresses representing users
        let admin = @0xBABE;
        let initial_owner = @0xCAFE;
        let final_owner = @0xFACE;

        // first transaction to emulate module initialization
        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;
        {
            init(test_scenario::ctx(scenario));
        };
        // second transaction executed by admin to create the sword
        test_scenario::next_tx(scenario, admin);
        {
            let forge = test_scenario::take_from_sender<Forge>(scenario);
            // create the sword and transfer it to the initial owner
            sword_create(42, 7, initial_owner, test_scenario::ctx(scenario));
            test_scenario::return_to_sender(scenario, forge)
        };
        // third transaction executed by the initial sword owner
        test_scenario::next_tx(scenario, initial_owner);
        {
            // extract the sword owned by the initial owner
            let sword = test_scenario::take_from_sender<Sword>(scenario);
            // transfer the sword to the final owner
            transfer::transfer(sword, final_owner);
        };
        // fourth transaction executed by the final sword owner
        test_scenario::next_tx(scenario, final_owner);
        {

            // extract the sword owned by the final owner
            let sword = test_scenario::take_from_sender<Sword>(scenario);
            // verify that the sword has expected properties
            assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
            // return the sword to the object pool (it cannot be simply "dropped")
            test_scenario::return_to_sender(scenario, sword)
        };
        test_scenario::end(scenario_val);
    }
```

Let us now dive into some details of the new testing function. The
first thing we do is to create some addresses that represent users
participating in the testing scenario. (We assume that we have one game
admin user and two regular users representing players.) We then create
a scenario by starting the first transaction on behalf of the admin
address that creates a sword and transfers its ownership to the
initial owner.

The second transaction is executed by the initial owner (passed as an
argument to the `test_scenario::next_tx` function) who then transfers
the sword it now owns to its final owner. Please note that in *pure
Move* we do not have the notion of Sui storage and, consequently, no
easy way for the emulated Sui transaction to retrieve it from
storage. This is where the `test_scenario` module comes to help - its
`take_from_sender` function makes an object of a given type (in this case
of type `Sword`) owned by an address executing the current transaction
available for manipulation by the Move code. (For now, we assume that
there is only one such object.) In this case, the object retrieved
from storage is transferred to another address.

> **Important:** Transaction effects, such as object creation/transfer become visible only after a
> given transaction completes. For example, if the second transaction in our running example created
> a sword and transferred it to the admin's address, it would become available for retrieval
> from the admin's address (via `test_scenario`s `take_from_sender` or `take_from_address`
> functions) only in the third transaction.

The final transaction is executed by the final owner - it retrieves
the sword object from storage and checks if it has the expected
properties. Remember, as described in
[testing a package](build-test.md#testing-a-package), in the *pure Move* testing
scenario, once an object is available in Move code (e.g., after its
created or, in this case, retrieved from emulated storage), it cannot simply
disappear.

In the *pure Move* testing function, we handled this problem
by transferring the sword object to the fake address. But the
`test_scenario` package gives us a more elegant solution, which is
closer to what happens when Move code is actually executed in the
context of Sui - we can simply return the sword to the object pool
using the `test_scenario::return_to_sender` function.

We can now run the test command again and see that we now have two
successful tests for our module:

``` shell
BUILDING MoveStdlib
BUILDING Sui
BUILDING MyFirstPackage
Running Move unit tests
[ PASS    ] 0x0::my_module::test_sword_create
[ PASS    ] 0x0::my_module::test_sword_transactions
Test result: OK. Total tests: 2; passed: 2; failed: 0
```
