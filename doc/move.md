# Move Quick Start

Welcome to the Sui tutorial for building smart contracts with
the [Move](https://github.com/diem/move) language. This tutorial
provides a brief explanation of the Move language and includes
concrete examples to demonstrate how Move can be used in Sui.


## Move

Move is an open source language for writing safe smart contracts. It
was originally developed at Facebook to power the [Diem](https://github.com/diem/diem)
blockchain. However, Move was designed as a platform-agnostic language
to enable common libraries, tooling, and developer communities across
blockchains with vastly different data and execution models. [Sui](../README.md),
[0L](https://github.com/OLSF/libra), and
[Starcoin](https://github.com/starcoinorg/starcoin) are using Move,
and there are also plans to integrate the language in several upcoming
and existing platforms (e.g.,
[Celo](https://www.businesswire.com/news/home/20210921006104/en/Celo-Sets-Sights-On-Becoming-Fastest-EVM-Chain-Through-Collaboration-With-Mysten-Labs)).


The Move language documentation is available in the
[Move Github](https://github.com/diem/move) repository and includes a
[tutorial](https://github.com/diem/move/blob/main/language/documentation/tutorial/README.md)
and a
[book](https://github.com/diem/move/blob/main/language/documentation/book/src/SUMMARY.md)
describing language features in detail. These are invaluable resources
to deepen your understanding of the Move language but not strict prerequisites
to following the Sui tutorial, which we strived to make self-contained.
Further, Sui does differ in some ways from Move, which we explore here.

In Sui, Move is used to define, create and manage programmable Sui
[objects](objects.md) representing user-level assets.  Sui
imposes additional restrictions on the code that can be written in
Move, effectively using a subset of Move (a.k.a. *Sui Move*), which
makes certain parts of the original Move documentation not applicable
to smart contract development in Sui (e.g., there is no concept of a
[script](https://github.com/diem/move/blob/main/language/documentation/book/src/modules-and-scripts.md#scripts)
in Sui Move). Consequently, it's best to simply follow this tutorial
and relevant Move documentation links provided in the tutorial.

Before we look at the Move code included with Sui, let's talk briefly
about Move code organization, which applies both to code included with
Sui and the custom code written by the developers.


## Move code organization

The main unit of Move code organization (and distribution) is a
_package_. A package consists of a set of _modules_ defined in separate
files with the `.move` extension. These files include Move functions and
type definitions. A package must include the `Move.toml` manifest file
describing package configuration, for example package metadata or
package dependencies. See
[Move.toml](https://github.com/diem/move/blob/main/language/documentation/book/src/packages.md#package-layout-and-manifest-syntax)
for more information about package manifest files.

The minimal package source directory structure looks as follows and
contains the manifest file and the `sources` subdirectory where one or
more module files are located (see
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/packages.md#package-layout-and-manifest-syntax)
for more information on package layout):

```
my_move_package
├── Move.toml
├── sources
    ├── M1.move
```

We are now ready to look at some Move code!

## First look at Move source code

The Sui platform includes _framework_ Move code that is needed to
bootstrap Sui operations. In particular, Sui supports multiple
user-defined coin types, which are custom assets define in the Move
language. Sui framework code contains the Coin module supporting
creation and management of custom coins. The Coin module is
the located in the
[sui_programmability/framework/sources/Coin.move](../sui_programmability/framework/sources/Coin.move)
file. As you can see the manifest file describing how to build the
package containing the Coin module is located, as expected, in the
[sui_programmability/framework/Move.toml](../sui_programmability/framework/Move.toml)
file.

Let's see how module definition in the Coin module file looks like
(let's not worry about the module content for now, though you can read
more about them in the Move
[book](https://github.com/diem/move/blob/main/language/documentation/book/src/modules-and-scripts.md#modules)
if immediately interested):

```rust
module Sui::Coin {
...
}
```

As we can see, when defining a module we specify the module name
(`Coin`), preceded by the name of the package where this module resides
(`Sui`). The combination of the package name and the module name
is used to uniquely identify a module in Move source code (e.g., to be
able to use if from other modules) - the package name is globally
unique, but different packages can contain modules with the same name.


In addition to having a presence at the source code level, as we
discussed [earlier](#move-code-organization), a package in Sui is also
a Sui object, and must have a unique numeric ID in addition to a
unique name, which is assigned in the manifest file:

``` 
[addresses]
Sui = "0x2"

[dev-addresses]
Sui = "0x2"
```

### Move structs

The Coin module defines the `Coin` struct type which can be used to
represent different types of user-defined coins as Sui objects:

``` rust
struct Coin<phantom T> has key, store {
    id: VersionedID,
    value: u64
}
```

Move's struct type is similar to struct types defined in other
programming languages, such as C or C++, and contains a name and a set
of typed fields. In particular, struct fields can be of a primitive
type, such as an integer type, or of a struct type (you can read more about
Move primitive types
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/SUMMARY.md#primitive-types)
and about Move structs
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/structs-and-resources.md)).


In order for a Move struct type to define a Sui object type such
`Coin`, its first field must be `id: VersionedID` (`VersionedID` is a
struct type defined in the ID
[module](../sui_programmability/framework/sources/ID.move)), and must
also have the `key` (which allows the object to be persisted in Sui's
global storage). Abilities of a Move struct are listed after the `has`
keyword in the struct definition, and their existence (or lack
thereof) helps enforcing various properties on a definition or on
instances of a given struct (you can read more about struct abilities
in Move
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/abilities.md))

The reason that the `Coin` struct can represent different types of
coin is that the struct definition is parameterized with a type
parameter. You can read more about Move type parameters (also known as
generics)
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/generics.md)
(and also about the optional `phantom` keyword
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/generics.md#phantom-type-parameters)),
but for now it suffices to say that when an instance of the `Coin`
struct is created, it can be passed an arbitrary concrete Move type
(e.g. another struct type) to distinguish different types of coins
from one another.

In particular, one type of custom coin already defined in Sui is
`Coin<GAS>`, which represents a token used to pay for gas used in Sui
computations - in this case, the concrete type used to parameterize the
`Coin` struct is the `GAS` struct in the GAS
[module](../sui_programmability/framework/sources/Coin.move):

``` rust
struct GAS has drop {}
```

We will show how to define and instantiate custom structs in the
[section](#writing-a-package) describing how to write a simple
Move package.

### Move functions

Similarly to other popular programming languages, the main unit of
computation in Move is a function. Let us look at one of the simplest
functions defined in the Coin
[module](../sui_programmability/framework/sources/Coin.move), that is
the `value` function.

``` rust
public fun value<T>(self: &Coin<T>): u64 {
    self.value
}
```

This _public_ function can be called by functions in other modules to
return the unsigned integer value currently stored in a given
instance of the `Coin` struct, (direct access to fields of a struct is
allowed only within the module defining a given struct as described
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/structs-and-resources.md#privileged-struct-operations)). The
body of the function simply retrieves the `value` field from the
`Coin` struct instance parameter and returns it. Please note that the
coin parameter is a read-only reference to the `Coin` struct instance,
indicated by the `&` preceding the parameter type. Move's type system
enforces an invariant that struct instance arguments passed by
read-only references (as opposed to mutable references) cannot be
modified in the body of a function (you can read more about Move
references
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/references.md#references)).


We will show how to call Move functions from other functions and how
to define the new ones in the [section](#writing-a-package)
describing how to write a simple Move package.


In addition to functions callable from other functions, however, the
Sui flavor of the Move language also defines so called _entry
functions_ that can be called directly from Sui (e.g., from a Sui
wallet application that can be written in a different language) and
must satisfy a certain set of properties.

#### Entry functions

One of the basic operations in Sui is transfer of gas objects between
[addresses](overview.md) representing individual users, and one of the
simplest entry functions is defined in the GAS
[module](../sui_programmability/framework/sources/GAS.move) to
implement gas object transfer (let's not worry about the function body
for now - since the function is part of Sui framework, you can trust
that it will do what it is intended to do):

```rust
public fun transfer(c: Coin::Coin<GAS>, recipient: vector<u8>, _ctx: &mut TxContext) {
    ...
}
```

In general, an entry function, must satisfy the following properties:

- must be public
- must have no return value
- its parameters are ordered as follows:
  - one or more Sui objects (or vectors of objects),
  - one or more primitive types (or vectors of such types)
  - mutable reference to an instance of the `TxContext` struct
  defined in the TxContext
  [module](../sui_programmability/framework/sources/TxContext.move)

More, concretely, the `transfer` function is public, has no return
value, and has three parameters:

- `c` - it represents a gas object whose ownership is to be
  transferred
- `recipient` - it is the address of the intended recipient,
  represented as a vector (built-in `vector` type) of 8-bit integers
  (built-in `u8` type) - you can read more about built-in primitive
  types like these
  [here](https://github.com/diem/move/blob/main/language/documentation/book/src/SUMMARY.md#primitive-types)
- `_ctx` - it is a mutable reference to an instance of the `TxContext`
  struct (in this particular case, this parameter is not actually used
  in the function's body as indicated by its name starting with `_`)
  
You can see how the `transfer` function is called from a sample Sui
wallet [here](wallet.md#calling-move-code).


## Writing a package

In order to be able to build a Move package and run code defined in
this package, please make sure that you have cloned the Sui repository
to the current directory and built Sui binaries as described
[here](wallet.md#build-the-binaries).

The directory structure used in this tutorial should at the moment
look as follows (assuming Sui has been cloned to a directory called
"sui"):

```
current_directory
├── sui
```

For convenience, please also make sure the path to Sui binaries
(sui/target/release), including the sui-move command used throughout
this tutorial, is part of your system path.

We can now proceed to creating a package directory structure and an
empty manifest file following the Move code organization described
[earlier](#move-code-organization):

``` shell
mkdir -p my_move_package/sources
touch my_move_package/Move.toml
```

The directory structure should now look as follows:

```
current_directory
├── sui
├── my_move_package
    ├── Move.toml
    ├── sources
        ├── M1.move
```


Let us assume that our module is part of an implementation of a
fantasy game set in medieval times, where heroes roam the land slaying
beasts with their trusted swords to gain prizes. All of these entities
will be represented by Sui objects, in particular we want a sword to
be an upgrade-able asset that can be shared between different players. A
sword asset can be defined similarly to another asset we are already
[familiar](#first-look-at-move-source-code) with, that is a `Coin`
struct type. Let us put the following module and struct definitions in
the M1.move file:

``` rust
module MyMovePackage::M1 {
    use Sui::ID::VersionedID;

    struct Sword has key, store {
        id: VersionedID,
        magic: u64,
        strength: u64,
    }
}
```

Since we are developing a fantasy game, in addition to the mandatory
`id` field as well as `key` and `store` abilities (same as in the
`Coin` struct), our asset has both `magic` and `strength` fields
describing its respective attribute values. Please note that we need
to import the ID
[package](../sui_programmability/framework/sources/ID.move) from Sui
framework to gain access to the `VersionedID` struct type defined in
this package.

If we want to access sword attributes from a different package, we
need to add accessor functions to our module similar to the `value`
function in the Coin package described [earlier](#move-functions):

``` rust
    public fun magic(self: &Sword): u64 {
        self.magic
    }

    public fun strength(self: &Sword): u64 {
        self.strength
    }
```

In order to build a package containing this simple module, we need to
put some required metadata into the Move.toml file, including package
name, package version, local dependency path to locate Sui framework
code, and (described [earlier]((#first-look-at-move-source-code)))
package numeric ID (which must be 0x0 for user-defined modules to
facilitate package [publishing](wallet.md#package-publishing)).

```
[package]
name = "MyMovePackage"
version = "0.0.1"

[dependencies]
Sui = { local = "../fastnft/sui_programmability/framework/" }

[addresses]
MyMovePackage = "0x0"

[dev-addresses]
MyMovePackage = "0x0"
```

We can now go to the directory containing our package and build it (no
output is expected on a successful build):

``` shell
cd my_move_package
sui-move build
```

Now that we have designed our asset and its accessor functions, let us
test the code we have written.

## Testing a package

Sui included support for Move's testing
[framework](https://github.com/diem/move/blob/main/language/documentation/book/src/unit-testing.md)
that allows you to write unit tests to test Move code much like test
frameworks for other languages (e.g., Rust's built-in testing
[framework](https://doc.rust-lang.org/rust-by-example/testing/unit_testing.html)
or the JUnit [framework](https://junit.org/) for Java).

An individual Move unit test is encapsulated in a public function that
has no parameters, no return values, and has the `#[test]`
annotation - such functions will be executed by the testing framework
upon executing the following command (in the `my_move_package`
directory as per our running example):

``` shell
sui-move test
```

If we execute this command for the package we created in the previous
[section](#writing-a-package), we will see the following output
indicating, unsurprisingly, that no tests have ran because we have not
written any yet!

``` shell
BUILDING MoveStdlib
BUILDING Sui
BUILDING MyMovePackage
Running Move unit tests
Test result: OK. Total tests: 0; passed: 0; failed: 0
```

Let us write a simple test function and insert it into the M1.move
file:

``` rust
    #[test]
    public fun test_sword_create() {
        use Sui::TxContext;
        
        // create a dummy TxContext for testing
        let ctx = TxContext::dummy();
        
        // create a sword
        let sword = Sword {
            id: TxContext::new_id(&mut ctx),
            magic: 42,
            strength: 7,                
        };
        
        // check if accessor functions return correct values
        assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
    }
```

The code of the unit test function is largely self-explanatory - we
create a dummy instance of the `TxContext` struct needed to create
unique identifier of our sword object, then create the sword itself,
and finally call its accessor functions to verify that they return
correct values. Please note that the dummy context is passed to the
`TxContext::new_id` function as a mutable reference argument (`&mut`)
and the sword itself is passed to its accessor functions as read-only
reference argument.

Now that we have written a test, let's try to run the tests again
then:

``` shell
sui-move test
```

After running the test command, however, instead of a test result we
get a compilation error:

``` shell
error[E06001]: unused value without 'drop'
   ┌─ ./sources/M1.move:34:65
   │  
 4 │       struct Sword has key, store {
   │              ----- To satisfy the constraint, the 'drop' ability would need to be added here
   ·  
27 │           let sword = Sword {
   │               ----- The local variable 'sword' still contains a value. The value does not have the 'drop' ability and must be consumed before the function returns
   │ ╭─────────────────────'
28 │ │             id: TxContext::new_id(&mut ctx),
29 │ │             magic: 42,
30 │ │             strength: 7,                
31 │ │         };
   │ ╰─────────' The type 'MyMovePackage::M1::Sword' does not have the ability 'drop'
   · │
34 │           assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
   │                                                                   ^ Invalid return
```

This error message looks quite complicated, but it contains all the
information needed to understand what went wrong. What happened here
is that while writing the test, we accidentally stumbled upon one of
the Move language's safety features.

Please remember that the `Sword` struct represents a game asset
digitally mimicking a real-world item. At the same time, while a sword
in a real world cannot simply disappear (though it can be explicitly
destroyed), there is no such restriction on a digital one. In fact,
this is exactly what's happening in our test function - we create an
instance of a `Sword` struct that simply disappears at the end of the
function call. And this is the gist of the error message we are
seeing, and on of the solutions (as suggested in the message itself),
is to add the `drop` ability to the definition of the `Sword` struct,
which would allow instances of this struct disappear (be
"dropped"). Arguably, being able to "drop" a valuable asset is not an
asset property we would like to have, so another solution to our
problem is to transfer ownership of the sword.

In order to get our test to work, we then add the following line to
the beginning of our testing function to import the Transfer
[module](../sui_programmability/framework/sources/Transfer.move):

``` rust
        use Sui::Transfer;

```

We then use the Transfer module to transfer ownership of the sword to
a freshly created dummy address by adding the following lines to the
end of our test function:

``` rust
        // create a dummy address and transfer the sword
        let dummy_address = @0xCAFE;        
        Transfer::transfer(sword, dummy_address);
```

We can now run the test command again and see that indeed a single
successful test has been ran:

``` shell
BUILDING MoveStdlib
BUILDING Sui
BUILDING MyMovePackage
Running Move unit tests
[ PASS    ] 0x0::M1::test_sword_create
Test result: OK. Total tests: 1; passed: 1; failed: 0
```

The testing example we have seen so far is largely "pure" Move and has
little to do with Sui beyond using some Sui packages, such as
`Sui::TxContext` and `Sui::Transfer`. While this style of testing is
already very useful for developers writing Move code for Sui, they may
also want to test additional Sui-specific features. In particular, a
Move call in Sui is encapsulated in a Sui
[transaction](https://github.com/MystenLabs/fastnft/blob/main/doc/transactions.md),
and a developer may wish to test interactions between different
transactions within a single test (e.g. one transaction creating an
object and the other one transferring it).

### Sui-specific testing

Sui-specific testing is supported via the `TestScenario`
[module](../sui_programmability/framework/sources/TestScenario.move)
that provides Sui-related testing functionality otherwise unavailable
in "pure" Move and its testing
[framework](https://github.com/diem/move/blob/main/language/documentation/book/src/unit-testing.md).

The main concept in the `TestScenario` is a scenario which emulates a
series of Sui transactions, each executed by a (potentially) different
user. At a high level, a developer writing a test starts the first
transaction using the `TestScenario::begin` function which takes an
address of the user executing this transaction as the first an only
argument, and return an instance of the `Scenario` struct representing
a scenario. An instance of the `Scenario` struct contains a
per-address object pool emulating Sui's object storage, with helper
functions provided to manipulate objects in the pool. Once the first
transaction is finished, subsequent transactions can be started using
the `TestScenario::next_tx` function that takes an instance of the
`Scenario` struct representing the current scenario and an address of
a (new) user as arguments.

Let us extend our running example with a multi-transaction test that
uses the `TestScenario` to test sword creation and transfer from the
point of view of Sui developer. First, let us create entry
[functions](#entry-functions) callable from Sui that implement sword
creation and transfer and put the into the M1.move file:

``` rust
    public fun sword_create(magic: u64, strength: u64, recipient: address, ctx: &mut TxContext) {
        use Sui::Transfer;
        use Sui::TxContext;
        // create a sword
        let sword = Sword {
            id: TxContext::new_id(ctx),
            magic: magic,
            strength: strength,
        };
        // transfer the sword
        Transfer::transfer(sword, recipient);
    }

    public fun sword_transfer(sword: Sword, recipient: address, _ctx: &mut TxContext) {
        use Sui::Transfer;
        // transfer the sword
        Transfer::transfer(sword, recipient);
    }
```

The code of the new functions is self-explanatory and uses struct
creation and Sui-internal modules (`TxContext` and `Transfer`) in a
way similar to what we have seen in the previous sections. The
important part is for the entry functions to have correct signatures
as described [earlier](#entry-functions). In order for this code to
build, we need to add an additional import line at the module level
(as the first line in the module's main code block right before the
existing module-wide `ID` module import) to make `TxContext` struct
available for function definitions:

``` rust
    use Sui::TxContext::TxContext;
```

We can now build the module extended with the new functions, but still
have only one test defined. Let use change that by adding another test
function:

``` rust
    #[test]
    public fun test_sword_transactions() {
        use Sui::TestScenario;

        let admin = @0xBABE;
        let initial_owner = @0xCAFE;
        let final_owner = @0xFACE;

        // first transaction executed by admin
        let scenario = &mut TestScenario::begin(&admin);
        {
            // create the sword and transfer it to the initial owner
            sword_create(42, 7, initial_owner, TestScenario::ctx(scenario));
        };
        // second transaction executed by the initial sword owner
        TestScenario::next_tx(scenario, &initial_owner);
        {
            // extract the sword owned by the initial owner
            let sword = TestScenario::remove_object<Sword>(scenario);
            // transfer the sword to the final owner
            sword_transfer(sword, final_owner, TestScenario::ctx(scenario));
        };
        // third transaction executed by the final sword owner
        TestScenario::next_tx(scenario, &final_owner);
        {
            // extract the sword owned by the final owner
            let sword = TestScenario::remove_object<Sword>(scenario);
            // verify that the sword has expected properties
            assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
            // return the sword to the object pool (it cannot be simply "dropped")
            TestScenario::return_object(scenario, sword)
        }
    }
```

Let us now dive into some details of the new testing function. The
first thing we do is to create some addresses that represent users
participating in the testing scenario (we assume that we have one game
admin user and two regular users representing players). We then create
a scenario by starting the first transaction on behalf of the admin
address that creates a sword and transfers its ownership to the
initial owner.

The second transaction is executed by the initial owner (passed as
argument to the `TestScenario::next_tx` function ) who then transfers
the sword it now owns the its final owner. Please note that in "pure"
Move we do not have the notion of Sui storage and, consequently, no
easy way for the emulated Sui transaction to retrieve it from
storage. This is where the `TestScenario` module comes to help - its
`remove_object` function makes an object of a given type (in this case
of type `Sword`) owned by an address executing the current transaction
available for manipulation by the Move code (for now we assume that
there is only one such object). In this case, the object retrieved
from storage is transferred to another address.

The final transaction is executed by the final owner - it retrieves
the sword object from storage and checks if it has the expected
properties. Please remember that, as described
[earlier](#testing-a-package) in the "pure" Move testing scenario,
once an object is available in Move code (e.g., after its created or,
in this case, retrieved from emulated storage), it cannot simply
disappear. In the "pure" Move testing function we handled this problem
by transferring the sword object to the fake address but the
`TestScenario` package gives us a more elegant solution which is
closer to what happens when Move code is actually executed in the
context of Sui - we can simply return the sword to the object pool
using `TestScenario::return_object` function.

We can now run the test command again and see that we now have two
successful tests for our module:

``` shell
BUILDING MoveStdlib
BUILDING Sui
BUILDING MyMovePackage
Running Move unit tests
[ PASS    ] 0x0::M1::test_sword_create
[ PASS    ] 0x0::M1::test_sword_transactions
Test result: OK. Total tests: 2; passed: 2; failed: 0
```

## Publishing a package

For functions in a Move package to actually be callable from Sui
(rather than for Sui execution scenario to be emulated), the package
has to be _published_ to Sui's [distributed
ledger](SUMMARY.md#architecture)
where it is represented as a Sui object. At this point, however, the
sui-move command does not support package publishing. In fact it is
not clear if it even makes sense to accommodate package publishing,
which happens once per package creation, in the context of a unit
testing framework. Instead, one can use a sample Sui wallet to
[publish](wallet.md#package-publishing) Move code and to
[call](wallet.md#calling-move-code). Please see the wallet
[documentation](wallet.md#package-publishing) for a description on how
to publish the package we have [written](#writing-a-package) as as
part of this tutorial.
