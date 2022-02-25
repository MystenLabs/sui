# Move Quick Start

Welcome to the Sui tutorial focusing on building smart contracts using
the Move language. This tutorial will provide a brief explanation of
the Move language but will mostly focus on using concrete examples to
demonstrate how Move can be used in the context of Sui.


## Move

Move is an [open-source](https://github.com/diem/move) language for
writing safe smart contracts. It was originally developed at Facebook
to power the [Diem](https://github.com/diem/diem) blockchain. However,
Move was designed as a platform-agnostic language to enable common
libraries, tooling, and developer communities across blockchains with
vastly different data and execution models. Sui,
[0L](https://github.com/OLSF/libra), and
[StarCoin](https://github.com/starcoinorg/starcoin) are using Move,
and there are also plans to integrate the language in several upcoming
and existing platforms (e.g.,
[Celo](https://www.businesswire.com/news/home/20210921006104/en/Celo-Sets-Sights-On-Becoming-Fastest-EVM-Chain-Through-Collaboration-With-Mysten-Labs)).


The Move language documentation is available in the Move Github
repository, and includes a
[tutorial](https://github.com/diem/move/blob/main/language/documentation/tutorial/README.md)
and a
[book](https://github.com/diem/move/blob/main/language/documentation/book/src/SUMMARY.md)
describing language features in detail. These are invaluable resources
to deepen your understanding of the Move language, but they are not a
strict prerequisite to following the Sui tutorial which we strived to
make self-contained.

In Sui, Move is used to define, create and manage programmable Sui
[objects](https://github.com/MystenLabs/fastnft/blob/main/doc/objects.md#objects)
representing user-level assets.  Sui imposes additional restrictions
on the code that can be written in Move, effectively using a subset of
Move (a.k.a. Sui Move), which makes certain parts of the original Move
documentation not applicable to smart contract development in Sui.

Before we look at the Move code included with Sui, let's talk briefly
about Move code organization, which applies both to code included with
Sui and the custom code written by the developers.


## Move code organization

The main unit of Move code organization (and distribution) is a
_package_. A package consists of set of _modules_ defined in separate
files with the .move extension, which include Move functions and type
definitions. A package must include the Move.toml manifest file
describing package configuration, for example package metadata or
package dependencies (more information about package manifest files
can be found [here](Move.toml)).

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
bootstrap Sui operations. In particular, unlike traditional blockchain
systems (e.g., Bitcoin or Ethereum), Sui supports multiple
user-defined coin types, which are custom assets define in the Move
language. Sui framework code contains the Coin module supporting
creation and management of custom coins. The Coin module is located in
the located in the
[sui_programmability/framework/sources/Coin.move](../sui_programmability/framework/sources/Coin.move)
file. As you can see the manifest file for the FastX package
containing the Coin module is located, as expected, in the
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
FastX = "0x2"

[dev-addresses]
FastX = "0x2"
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
`Coin`, its definition must include the `id` field of `VersionedID`
type (which struct type defined in the ID
[module](../sui_programmability/framework/sources/ID.move)), and must
also have the `key` ability used to enforce existence of the `id`
field. Abilities of a Move struct are listed after the `has` keyword
in the struct definition, and their existence (or lack thereof) helps
enforcing various properties on a definition or on instances of a
given struct - for example the `store` ability allows instances of a
given struct to be persisted in Sui's distributed ledger (you can read
more about struct abilities in Move
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
[section](#writing-simple-package) describing how to write a simple
Move package.

### Move functions

Similarly to other popular programming languages, the main unit
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
only allowed within the module defining a given struct as described
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/structs-and-resources.md#privileged-struct-operations)). The
body of the function simply retrieves the `value` field from the
`Coin` struct instance parameter and returns it. Please note that the
coin parameter is a read-only reference to the `Coin` struct instance,
indicated by the `&` preceding the parameter type. Move's type system
enforces an invariant that struct instances arguments passes by
read-only references (as opposed to mutable references) cannot be
modified in the body of a function (you can read more about Move
references
[here](https://github.com/diem/move/blob/main/language/documentation/book/src/references.md#references)).


We will show how to call Move functions from other functions and how
and define the new ones in the [section](#writing-simple-package)
describing how to write a simple Move package.


In addition to functions callable from other functions, however, the
Sui flavor of the Move language also defines so called _entry
functions_ that can be called directly from Sui (e.g., from a Sui
wallet application that can be written in a different language), and
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
value, and has 3 parameters:

- `c` - it represents a gas object whose ownership is to be
  transferred
- `recipient` - it is the address of the intended recipient,
  represented as a vector (built-in `vector` type) of 8-bit integers
  (built-in `u8` type) - you can read more about built-in primitive
  types lie these
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
(sui/target/release) is part of your system path.

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
    use FastX::ID::VersionedID;

    struct Sword has key, store {
        id: VersionedID,
        magic: u64,
        strength: u64,
    }
}
```

Since we are developing a fantasy game, in addition to the mandatory
`id` field as well as `key` and `store` abilities (same as in the
`Coin` struct), our asset has both `magic` and `strenght` fields
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
FastX = { local = "../fastnft/sui_programmability/framework/" }

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

TBD
