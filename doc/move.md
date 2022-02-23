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

More importantly, Sui imposes additional restrictions on the code that
can be written in Move, effectively using a subset of Move (aka Sui
Move), which makes certain parts of the original Move documentation
not applicable to smart contract development in Sui.

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
    ├── MyModule.move
```

We are now ready to look at some Move code!

## First look at Move source code

The Sui platform includes _framework_ Move code that is needed to
bootstrap Sui operations, for example to create and manipulate gas
objects. In particular the gas object is defined in the GAS module
located in the
[sui_programmability/framework/sources/GAS.move](../sui_programmability/framework/sources/GAS.move)
file. As you can seem the manifest file for the package containing the
GAS module is located, as expected, in the
[sui_programmability/framework/Move.toml](../sui_programmability/framework/Move.toml)
file.

Let's see how module definition in the GAS module file looks like
(let's not worry about the module content for now, though you can read
more about them in the Move
[book](https://github.com/diem/move/blob/main/language/documentation/book/src/modules-and-scripts.md#modules)
if immediately interested):

```rust
module FastX::GAS {
...
}
```

As we can see, when defining a module we specify the module name
(`GAS`), preceded by the name of the package where this module resides
(`FastX`). The combination of the package name and the module name
is used to uniquely identify a module in Move source code (e.g., to be
able to use if from other modules) - the package name is globally
unique, but different packages can contain modules with the same name.


In addition to having a presence at the source code level, as we
discussed [earlier](#move-code-organization), a package in Sui is also
an object, and must have a unique numeric ID in addition to a unique
name, so that it can be identified by the Sui platform. For the
framework packages this address is is assigned in the manifest file:

``` 
[addresses]
FastX = "0x2"
```

### First look at Move function definition

One of the basic operations in Sui is transfer of gas objects between
[addresses](overview.md) representing individual users. Here is the
transfer function definition in the GAS
[module](../sui_programmability/framework/sources/GAS.move):

```rust
public fun transfer(c: Coin::Coin<GAS>, recipient: vector<u8>, _ctx: &mut TxContext) {
    Coin::transfer(c, Address::new(recipient))
}
```

It is a public function called `transfer` with 3 arguments:

- `c` - it represents a gas object whose ownership is to be
  transferred; a gas object _type_ (`Coin::Coin<GAS>`) is `Coin`
  struct (you can read more about Move structs
  [here](https://github.com/diem/move/blob/main/language/documentation/book/src/structs-and-resources.md#structs-and-resources))
  defined in the Coin
  [module](../sui_programmability/framework/sources/Coin.move)
  parameterized with another struct `GAS` defined in the GAS
  [module](../sui_programmability/framework/sources/GAS.move) (you can
  read more about generic types and how they can be used to
  parameterize other types
  [here](https://github.com/diem/move/blob/main/language/documentation/book/src/generics.md#generics).
- `recipient` - it is the address of the intended recipient,
  represented as a vector (built-in `vector` type) of 8-bit integers
  (built-in `u8` type) - you can read more about built-in primitive
  types lie these
  [here](https://github.com/diem/move/blob/main/language/documentation/book/src/SUMMARY.md#primitive-types)
- `_ctx` - it is a mutable reference to an instance of the `TxContext`
  struct defined in the TxContext
  [module](../sui_programmability/framework/sources/TxContext.move)
  (you can read more about references
  [here](https://github.com/diem/move/blob/main/language/documentation/book/src/references.md)
  but for now we do not have to worry about this parameter too much as
  it is unused, which is indicated by its name starting with `_`)
  
The `transfer` function calls another function defined in the Coin
module that ultimately (through a series of other calls) implements
actual logic of transferring an instance of the `Coin` struct to a
different owner (`Address::new` function is responsible for creating
an internal representation of the recipient's address). The good thing
is that (at least for now) you don't have to worry about how this
logic is implemented - you can simply trust that framework functions
included with Sui genesis will do what they are intended to do
correctly.

Now that we have some understanding of Move code organization and of
how Move functions are defined, let us write a simple package that
will simply call into existing Sui framework code.

## Writing Simple Package

Following the Move code organization described
[earlier](#move-code-organization), let us first create the package
directory structure and create an empty manifest file:

``` shell
mkdir -p my_move_package/sources
touch my_move_package/Move.toml
```

TBD
