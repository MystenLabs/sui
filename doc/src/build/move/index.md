---
title: Write Smart Contracts with Sui Move
---

Welcome to the Sui tutorial for building smart contracts with
the [Move](https://github.com/MystenLabs/awesome-move) language.
This tutorial provides a brief explanation of the Move language and
includes concrete examples to demonstrate how Move can be used in Sui.

## Quick links

* [Why Move?](../../learn/why-move.md) - Quick links to external Move resources and a comparison with Solidity
* [How Sui Move differs from Core Move](../../learn/sui-move-diffs.md) - Highlights the differences between the core Move language and the Move we use in Sui
* [Programming Objects Tutorial Series](../../build/programming-with-objects/index.md) - Tutorial series that walks through all the powerful ways to interact with objects in Sui Move.

## Move

Move is an open source language for writing safe smart contracts. It
was originally developed at Facebook to power the [Diem](https://github.com/diem/diem)
blockchain. However, Move was designed as a platform-agnostic language
to enable common libraries, tooling, and developer communities across
blockchains with vastly different data and execution models. [Sui](https://github.com/MystenLabs/sui/blob/main/README.md),
[0L](https://github.com/OLSF/libra), and
[Starcoin](https://github.com/starcoinorg/starcoin) are using Move,
and there are also plans to integrate the language in several upcoming
and existing platforms (e.g.,
[Celo](https://www.businesswire.com/news/home/20210921006104/en/Celo-Sets-Sights-On-Becoming-Fastest-EVM-Chain-Through-Collaboration-With-Mysten-Labs)).


The Move language documentation is available in the
[Move GitHub](https://github.com/move-language/move) repository and includes a
[tutorial](https://github.com/move-language/move/blob/main/language/documentation/tutorial/README.md)
and a
[book](https://github.com/move-language/move/blob/main/language/documentation/book/src/SUMMARY.md)
describing language features in detail. These are invaluable resources
to deepen your understanding of the Move language but not strict prerequisites
to following the Sui tutorial, which we strived to make self-contained.
Further, Sui does differ in some ways from Move, which we explore here.

In Sui, Move is used to define, create and manage programmable Sui
[objects](../objects.md) representing user-level assets.  Sui
imposes additional restrictions on the code that can be written in
Move, effectively using a subset of Move (a.k.a. *Sui Move*), which
makes certain parts of the original Move documentation not applicable
to smart contract development in Sui. Consequently, it's best to simply follow this tutorial
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
[Move.toml](https://github.com/move-language/move/blob/main/language/documentation/book/src/packages.md#movetoml)
for more information about package manifest files.

The minimal package source directory structure looks as follows and
contains the manifest file and the `sources` subdirectory where one or
more module files are located:

```
my_move_package
├── Move.toml
├── sources
    ├── m1.move
```

See
[Package Layout and Manifest Syntax](https://github.com/move-language/move/blob/main/language/documentation/book/src/packages.md#package-layout-and-manifest-syntax)
for more information on package layout.

We are now ready to look at some Move code! You can either keep
reading for an introductory description of the main
Move language constructs or you can jump straight into the code by [writing a simple Move package](write-package.md), and checking out additional code [examples](../../explore/examples.md).

## First look at Move source code

The Sui platform includes _framework_ Move code that is needed to
bootstrap Sui operations. In particular, Sui supports multiple
user-defined coin types, which are custom assets defined in the Move
language. Sui framework code contains the `Coin` module supporting
creation and management of custom coins. The `Coin` module is
located in the
[coin.move](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/coin.move)
file. As you would expect, the manifest file describing how to build the
package containing the `Coin` module is located in the corresponding
[Move.toml](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/Move.toml)
file.

Let's see how module definition appears in the `Coin` module file:

```rust
module sui::coin {
...
}
```

(Let's not worry about the rest of the module contents for now; you can
read more about
[modules](https://github.com/move-language/move/blob/main/language/documentation/book/src/modules-and-scripts.md#modules)
in the Move book later.)

> **Important:** In Sui Move, package names are always in CamelCase, while
> the address alias is lowercase, for examples `sui = 0x2` and `std = 0x1`.
> So: `Sui` = name of the imported package (Sui = sui framework), `sui` = address
> alias of 0x2, `sui::sui` = module sui under the address 0x2, and
> `sui::sui::SUI` = type in the module above.

As we can see, when defining a module we specify the module name
(`Coin`), preceded by the name of the package where this module resides
(`Sui`). The combination of the package name and the module name
is used to uniquely identify a module in Move source code (e.g., to be
able to use if from other modules). The package name is globally
unique, but different packages can contain modules with the same name.
Module names are not unique, but combined with unique package name renders
a unique combination.

For example, if you have package "P" that has been published, you cannot
publish another package named "P". At the same time you can have module
"P1::M1", "P2::M1", and "P1::M2" but not another, say, "P1::M1" in the system
at the same time.

In addition to having a presence at the source code level, as we
discussed in [Move code organization](#move-code-organization), a
package in Sui is also a Sui object and must have a unique numeric
ID in addition to a unique name, which is assigned in the manifest
file:

```
[addresses]
sui = "0x2"
```

### Move structs

The `Coin` module defines the `Coin` struct type that can be used to
represent different types of user-defined coins as Sui objects:

``` rust
struct Coin<phantom T> has key, store {
    info: Info,
    value: u64
}
```

Move's struct type is similar to struct types defined in other
programming languages, such as C or C++, and contains a name and a set
of typed fields. In particular, struct fields can be of a primitive
type, such as an integer type, or of a struct type.

You can read more about
Move [primitive types](https://github.com/move-language/move/blob/main/language/documentation/book/src/SUMMARY.md#primitive-types)
and [structs](https://github.com/move-language/move/blob/main/language/documentation/book/src/structs-and-resources.md)
in the Move book.

In order for a Move struct type to define a Sui object type such as
`Coin`, its first field must be `info: Info`, which is a
struct type defined in the
[object module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/object.move). The
Move struct type must
also have the `key` ability, which allows the object to be persisted
in Sui's global storage. Abilities of a Move struct are listed after
the `has` keyword in the struct definition, and their existence (or
lack thereof) helps enforcing various properties on a definition or on
instances of a given struct.

You can read more about struct
[abilities](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md)
in the Move book.

The reason that the `Coin` struct can represent different types of
coin is that the struct definition is parameterized with a type
parameter. When an instance of the `Coin` struct is created, it can
be passed an arbitrary concrete Move type (e.g. another struct type)
to distinguish different types of coins from one another.

Learn about Move type parameters known as
[generics](https://github.com/move-language/move/blob/main/language/documentation/book/src/generics.md)
and also about the optional
[phantom keyword](https://github.com/move-language/move/blob/main/language/documentation/book/src/generics.md#phantom-type-parameters))
at your leisure.

In particular, one type of custom coin already defined in Sui is
`Coin<SUI>`, which represents a token used to pay for Sui
computations (more generally known as _gas_) - in this case, the concrete type used to parameterize the
`Coin` struct is the `SUI` struct in the
[SUI module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/sui.move):

``` rust
struct SUI has drop {}
```

We will show how to define and instantiate custom structs in the
section describing how to
[write a simple Move package](write-package.md).

### Move functions

Similarly to other popular programming languages, the main unit of
computation in Move is a function. Let us look at one of the simplest
functions defined in the
[Coin module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/coin.move), that is
the `value` function.

``` rust
public fun value<T>(self: &Coin<T>): u64 {
    self.value
}
```

This _public_ function can be called by functions in other modules to
return the unsigned integer value currently stored in a given
instance of the `Coin` struct. Direct access to fields of a struct is
allowed only within the module defining a given struct as described in
[Privileged Struct Operations](https://github.com/move-language/move/blob/main/language/documentation/book/src/structs-and-resources.md#privileged-struct-operations).
The body of the function simply retrieves the `value` field from the
`Coin` struct instance parameter and returns it. Note that the
coin parameter is a read-only reference to the `Coin` struct instance,
indicated by the `&` preceding the parameter type. Move's type system
enforces an invariant that struct instance arguments passed by
read-only references (as opposed to mutable references) cannot be
modified in the body of a function.

You can read more about Move
[references](https://github.com/move-language/move/blob/main/language/documentation/book/src/references.md#references) in the Move book.

We will show how to call Move functions from other functions and how
to define the new ones in the section describing how to
[write a simple Move package](write-package.md).


In addition to functions callable from other functions, however, the
Sui flavor of the Move language also defines so called _entry
functions_ that can be called directly from Sui (e.g., from a Sui
application that can be written in a different language) and
must satisfy a certain set of properties.

#### Entry functions

One of the basic operations in Sui is transfer of gas objects between
[addresses](https://github.com/move-language/move/blob/main/language/documentation/book/src/address.md)
representing individual users. And one of the
simplest entry functions is defined in the
[SUI module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/sui.move)
to implement gas object transfer:

```rust
public entry fun transfer(c: coin::Coin<SUI>, recipient: address, _ctx: &mut TxContext) {
    ...
}
```

(Let's not worry about the function body
for now - since the function is part of Sui framework, you can trust
that it will do what it is intended to do.)

In general, an entry function, must satisfy the following properties:

- have the `entry` modifier
  - Note: The visibility does not matter. The function can be `public`, `public(friend)`, or internal.
- have no return value
- (optional) have a mutable reference to an instance of the `TxContext` struct
  defined in the [TxContext module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/tx_context.move) as the last parameter

More concretely, the `transfer` function is public, has no return
value, and has three parameters:

- `c` - represents a gas object whose ownership is to be
  transferred
- `recipient` - the [address](https://github.com/move-language/move/blob/main/language/documentation/book/src/address.md)
   of the intended recipient
- `_ctx` - a mutable reference to an instance of the `TxContext`
  struct (in this particular case, this parameter is not actually used
  in the function's body as indicated by its name starting with `_`)
  - Note that since it is unused, the parameter could be removed. The mutable reference to the `TxContext` is optional for entry functions.

You can see how the `transfer` function is called from a Sui
CLI client in [Calling Move code](../cli-client.md#calling-move-code).
