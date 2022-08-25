---
title: Write a Sui Move Package
---

## 

In order to build a Move package and run code defined in
this package, first [install Sui binaries](../install.md#binaries) and
[clone the repository](../install.md#source-code) as this tutorial assumes
you have the Sui repository source code in your current directory.

Refer to the code example developed for this tutorial in the
[m1.move](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/move_tutorial/sources/m1.move) file.

The directory structure used in this tutorial should at the moment
look as follows (assuming Sui has been cloned to a directory called
"sui"):

```
current_directory
├── sui
```

For convenience, make sure the path to Sui binaries
(`~/.cargo/bin`), including the `sui` command used throughout
this tutorial, is part of your system path:

```
$ which sui
```

### Creating the directory structure

Now proceed to creating a package directory structure in the current
directory, parallel to the `sui` repository. It will contain an
empty manifest file and an empty module source file following the
[Move code organization](../move/index.md#move-code-organization)
described earlier.

So from the same directory containing the `sui` repository create a
parallel directory to it by running:

``` shell
$ mkdir -p my_move_package/sources
touch my_move_package/sources/m1.move
touch my_move_package/Move.toml
```

The directory structure should now be (please note that directories at the same indentation level in the figure below should also be at the same level in the file system):

```
current_directory
├── sui
├── my_move_package
    ├── Move.toml
    ├── sources
        ├── m1.move
```

### Defining the package

Let us assume that our module is part of an implementation of a
fantasy game set in medieval times, where heroes roam the land slaying
beasts with their trusted swords to gain prizes. All of these entities
will be represented by Sui objects; in particular, we want a sword to
be an upgradable asset that can be shared between different players. A
sword asset can be defined similarly to another asset we are already
familiar with from our
[First look at Move source code](../move/index.md#first-look-at-move-source-code). That
is a `Coin` struct type.


Let us put the following module and struct
definitions in the `m1.move` file:

``` rust
module my_first_package::m1 {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    struct Sword has key, store {
        id: UID,
        magic: u64,
        strength: u64,
    }
}
```

Since we are developing a fantasy game, in addition to the mandatory
`id` field as well as `key` and `store` abilities (same as in the
`Coin` struct), our asset has both `magic` and `strength` fields
describing its respective attribute values. Please note that we need
to import the
[Object package](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/object.move) from
Sui framework to gain access to the `Info` struct type defined
in this package.

If we want to access sword attributes from a different package, we
need to add accessor functions to our module similar to the `value`
function in the Coin package described in [Move
functions](#move-functions) (please make sure you add these functions,
and all the following code in this tutorial, in the scope of our
package - between curly braces starting and ending the package
definition):

``` rust
    public fun magic(self: &Sword): u64 {
        self.magic
    }

    public fun strength(self: &Sword): u64 {
        self.strength
    }
```

In order to build a package containing this simple module, we need to
put some required metadata into the `Move.toml` file, including package
name, package version, local dependency path to locate Sui framework
code, and package numeric ID, which must be `0x0` for user-defined modules
to facilitate [package publishing](../cli-client.md#publish-packages).

```
[package]
name = "MyFirstPackage"
version = "0.0.1"

[dependencies]
Sui = { local = "../sui/crates/sui-framework" }

[addresses]
my_first_package = "0x0"
```

See the [Move.toml](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/move_tutorial/Move.toml)
file used in our [end-to-end tutorial](../../explore/tutorials.md) for an example.
