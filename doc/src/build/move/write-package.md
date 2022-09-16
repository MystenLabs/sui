---
title: Write a Sui Move Package
---

##

In order to build a Move package and run code defined in this package, first [install Sui binaries](../install.md#binaries).

### Creating the package

First, create an empty Move package:

``` shell
$ sui move new my_first_package
```

This creates a skeleton Move project in the `my_first_package` directory. Let's take a look at the package manifest created by this command:

```shell
$ cat my_first_package/Move.toml
[package]
name = "my_first_package"
version = "0.0.1"

[dependencies]
Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework", rev = "devnet" }

[addresses]
my_first_package = "0x0"
sui = "0x2"
```

This file contains:
* Package metadata such as name and version (`[package]` section)
* Other packages that this package depends on (`[dependencies]` section). This package only depends on the Sui Framework, but other third-party dependencies should be added here.
* A list of *named addresses* (`[addresses]` section). These names can be used as convenient aliases for the given addresses in the source code.


### Defining the package

Let's start by creating a source file in the package:

``` shell
$ touch my_first_package/sources/my_module.move
```

and adding the following code to the `my_module.move` file:

```rust
module my_first_package::my_module {
    // Part 1: imports
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    // Part 2: struct definitions
    struct Sword has key, store {
        id: UID,
        magic: u64,
        strength: u64,
    }

    struct Forge has key, store {
        id: UID,
        swords_created: u64,
    }

    // Part 3: module initializer to be executed when this module is published
    fun init(ctx: &mut TxContext) {
        let admin = Forge {
            id: object::new(ctx),
            swords_created: 0,
        };
        // transfer the forge object to the module/package publisher
        transfer::transfer(admin, tx_context::sender(ctx));
    }

    // Part 4: accessors required to read the struct attributes
    public fun magic(self: &Sword): u64 {
        self.magic
    }

    public fun strength(self: &Sword): u64 {
        self.strength
    }

    public fun swords_created(self: &Forge): u64 {
        self.swords_created
    }

    // part 5: public/ entry functions (introduced later in the tutorial)
    // part 6: private functions (if any)
}
```

Let's break down the four different parts of this code:

1. Imports: these allow our module to use types and functions declared in other modules. In this case, we pull in imports from three different modules.

2. Struct declarations: these define types that can be created/destroyed by this module. Here the `key` *abilities* indicate that these structs are Sui objects that can be transferred between addresses. The `store` ability on the sword allows it to appear in fields of other structs and to be transferred freely.

3. Module initializer: this is a special function that is invoked exactly once when the module is published.

4. Accessor functions--these allow the fields of the fields of module's struct to be read from other modules.
