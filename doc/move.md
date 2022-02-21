# Dev Quick Start

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

## Setup

This tutorial uses Sui Wallet CLI (command-line interface) to
demonstrate capabilities of the Sui platform. Please see the main
[README](../README.md) for instructions on how to install Sui and how
to setup the Sui Wallet (use default configurations when following the
instructions). At the end of this setup step, you should have the
following items at your disposal, which together form so called
_genesis state_ of the Sui platform.

- 5 [addresses](overview.md) representing individual users, each
address initialized with 5 gas objects (all Sui [objects](objects.md),
including gas objects, are "owned" by addresses representing Sui
users)
- 4 Sui [authorities](authorities.md) running locally in the
`target/release` subdirectory of your local copy of the Sui
repository
- some essential Move code needed to bootstrap the Sui platform

The number of accounts, authorities and gas objects available has been
chosen somewhat arbitrarily.

Before we look at the Move code included with Sui, let's talk briefly
about Move code organization, which applies both to code included with
Sui and the custom code written by the developers.

## Move Code Organization

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

In order for a package to be available in Sui, that is for functions
defined in its modules to be callable from Sui or other Move
functions, the package must be _published_ (a published package
becomes a Sui object). We will discuss publishing
[later](#package-publishing) in this tutorial, but for now it suffices
that all packages available as part of the genesis state are
pre-published during Sui's initial setup.

## Your First Move Call

The genesis state of the Sui platform includes Move code that is
needed to initialize Sui operations, for example to create and
manipulate gas objects. In particular the gas object is defined in the
genesis GAS module located in the
[sui_programmability/framework/sources/GAS.move](../sui_programmability/framework/sources/GAS.move)
file. As you can seem the manifest file for the package containing the
GAS module is located, as expected, in the
[sui_programmability/framework/Move.toml](../sui_programmability/framework/Move.toml)
file.

### A Quick Look at the GAS Module

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
name, so that it can be identified by the Sui platform. For user-level
packages, this ID is assigned when the package is
[published](#package-publishing), but for the packages pre-published
during Sui setup it is assigned in the manifest file:

``` 
[addresses]
FastX = "0x2"
```

Since as part of Sui genesis we have user accounts populated with gas
objects available to us, for our first Move call, we will call a
function transferring a gas object from one account to another. Here
is the transfer function definition in the GAS
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
  defined in the Coin genesis
  [module](../sui_programmability/framework/sources/Coin.move)
  parameterized with another struct `GAS` defined in the GAS genesis
  [module](../sui_programmability/framework/sources/GAS.move) (you can
  read more about generic types and how they can be used to
  parameterize other types
  [here](https://github.com/diem/move/blob/main/language/documentation/book/src/generics.md#generics).
- `recipient` - it is the address of the intended recipient,
  represented as a vector (built-in `vector` type) of 8=bit integers
  (built-in `u8` type) - you can read more about built-in primitive
  types lie these
  [here](https://github.com/diem/move/blob/main/language/documentation/book/src/SUMMARY.md#primitive-types)
- `_ctx` - it is a mutable reference to an instance of the `TxContext`
  struct defined in the genesis TxContext
  [module](../sui_programmability/framework/sources/TxContext.move)
  (you can read more about references
  [here](https://github.com/diem/move/blob/main/language/documentation/book/src/references.md)
  but for now we do not have to worry about this parameter too much as
  it is unused, which is indicated by its name starting with `_`)
  
The `transfer` function calls another function defined in the Coin
module that ultimately (through a series of other calls) implements
actual logic of transferring an instance of the `Coin` struct to a
different owner (`Address::new` function is responsible for creating an
internal representation of the recipient's address). The good thing is
that (at least for now) you don't have to worry about how this logic
is implemented - you can simply trust that functions defined as part
of Sui genesis will do what they are intended to do correctly.

### Transferring Gas Objects with Move

Let's first see the user addresses available as part of Sui genesis
(accounts in Sui are identifier. We use the following Wallet CLI
command to see all user addresses:

``` shell
./wallet --no-shell addresses
```

When running this command, you should see a list of 5 addresses,
though the actual address values will most likely differ in your case
(as will other values, such as object IDs, in the later parts of this
tutorial). Consequently, **please do not copy and paste the actual
command from this tutorial as they are unlikely to work for you
verbatim**.

``` shell
Showing 5 results.
0523fc67f30e3922147877ca56ce36a41ba122623fee86043f5c9a05d2b3bde4
5986f0651a5329b90d1d76acd992021377684509909b23a9bbf79c4416afd9cf
ce3c1f3f3cbb5abf7cb492c31a162b58089d03a2e6057b88fd8228435c9d44e7
d346982dd3a61084c6f7f5af0f1b559cdf2921a3e76f403e85925b3dcf1d991d
dc3e8f72f84422ce3b332756520d7730e7a44b6720b0cd91eaf21bf65d56de3e
```

Let's also see the gas objects owned by the first address, which can
be accomplished with the following command listing all objects owned
by given address:

``` shell
./wallet --no-shell objects --address 0523fc67f30e3922147877ca56ce36a41ba122623fee86043f5c9a05d2b3bde4
```

When looking at the output, let's focus on the first column which
lists object IDs owned by this address (the rest of the input is
replaced with `...` below):

``` shell
1FD8DA0C56694229761E9A3DCE50C49AF2EA5DB1: ...
363D5BCAC9D5855122202B6B832B321D4256F22E: ...
7022F48406251C0D5AE4EBEBB4C7150F3D34E195: ...
771101CE95E5A774D94E172CD54178C124327EB6: ...
B80052DE4A17C0A61B27857A31A5CAC0EF01EF2F: ...
```

Now that we know which objects are owned by the address starting with
`0523`, we can transfer one of them to another address, say one
starting with `5986`. We can try any object, but for the sake of this
exercise, let's choose the last one on the list, that is one whose ID
is starting with `B800`.

We will perform the transfer by calling the `transfer` function from
the GAS module using the following Sui Wallet command:

``` shell
./wallet --no-shell call \
--function transfer \
--module GAS \
--package 0000000000000000000000000000000000000002 \
--object-args B80052DE4A17C0A61B27857A31A5CAC0EF01EF2F \
--pure-args x\"5986f0651a5329b90d1d76acd992021377684509909b23a9bbf79c4416afd9cf\" \
--gas 1FD8DA0C56694229761E9A3DCE50C49AF2EA5DB1 \
--gas-budget 1000 \
--sender 0523fc67f30e3922147877ca56ce36a41ba122623fee86043f5c9a05d2b3bde4
```

This a pretty complicated command so let's explain all its parameters
one-by-one:

- `--function` - name of the function to be called
- `--module` - name of the module containing the function
- `--package` - ID of the package object where the module containing
  the function is located (please
  [remember](#a-quick-look-at-the-gas-module) that the ID of the
  genesis FastX package containing the GAS module is defined in its
  manifest file, and is equal to 0x2, which is here extended to 20
  bytes expected by the system)
- `object-args` - a list of arguments of Sui object type (in this case
  there is only one representing the `c` parameter of the `transfer`
  function)
- `pure-args` - a list of arguments of Sui primitive types or vectors
  of such types (in this case there is only one representing the
  `recipient` parameter of the `transfer` function)
- `--gas` - an object containing gas that will be used to pay for this
  function call that is owned by the address initiating the `transfer`
  function call (i.e., address starting with `0523`) - we chose gas
  object whose ID starts with `1FD8` but we could have any object
  owned by this address as at this point the only objects in Sui are
  gas objects
- `--gas-budget` - a decimal value expressing how much gas we are
  willing to pay for the `transfer` call to be completed (the gas
  object may contain a lot more gas than 1000 units and we may want to
  prevent it being drained accidentally beyond what we are intended to
  pay)
- `--sender` - the address of the account initiating the function
  call, which also needs to own the object to be transferred
  
Please note that the third argument to the `transfer` function
representing `TxContext` does not have to be specified explicitly - it
is a required argument for all functions callable from Sui and is
auto-injected by the platform at the point of a function call.

The output of the call command is a bit verbose, but the important
information that should be printed at the end indicates objects
changes as a result of the function call (we again abbreviate the
output to only include the first column of the object description
containing its ID):

``` shell
...
Mutated Objects:
1FD8DA0C56694229761E9A3DCE50C49AF2EA5DB1 ...
B80052DE4A17C0A61B27857A31A5CAC0EF01EF2F ...
```

This output indicates that the gas object whose ID starts with `1FD8`
was updated to collect gas payment for the function call, and the
object whose ID starts with `B800` was updated as its owner had been
modified. We can confirm the latter (and thus a successful execution
of the `transfer` function) but querying objects that are now owned by
the sender (abbreviated output):

``` shell
./wallet --no-shell objects --address 0523fc67f30e3922147877ca56ce36a41ba122623fee86043f5c9a05d2b3bde4
Showing 4 results.
1FD8DA0C56694229761E9A3DCE50C49AF2EA5DB1: ...
363D5BCAC9D5855122202B6B832B321D4256F22E: ...
7022F48406251C0D5AE4EBEBB4C7150F3D34E195: ...
771101CE95E5A774D94E172CD54178C124327EB6: ...
```

We can now see that this address no longer owns the object whose IS
starts with `B800`. On the other hand, the recipient now owns 6
objects including the transferred one (in the fourth position):

``` shell
./wallet --no-shell objects --address 5986f0651a5329b90d1d76acd992021377684509909b23a9bbf79c4416afd9cf
Showing 6 results.
348B607E5C8B80524D6BF8275FB7F35267A7814E: ...
5852529FE26D138D7B6B9281ADBF29645D93543A: ...
87128A733E6F8AE432C2B928A432309FD1E70363: ...
B80052DE4A17C0A61B27857A31A5CAC0EF01EF2F: ...
C80707F7D1C8CBAC58BFD9A1EAD18199F0ECE931: ...
DC5530627AFBFFBB1F52B81F273A7B666B31CB85: ...
```

## Writing Simple Package

We assume here that our working directory is `target/release`
subdirectory of the Sui repository, as specified in the setup
[section](#setup) of this document.

Let us first create the package directory structure and create an
empty manifest file:

``` shell
mkdir -p my_move_package/sources
touch my_move_package/Move.toml
```
TBD

## Package Publishing

TBD

