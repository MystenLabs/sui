---
title: Basics
---

This section covers the main features of Sui Move.

## Move.toml

Every Move package has a *package manifest* in the form of a `Move.toml` file - it is placed in the [root of the package](../build/move/index.md#move-code-organization). The manifest itself contains a number of sections, primary of which are:

- `[package]` - includes package metadata such as name and author
- `[dependencies]` - specifies dependencies of the project
- `[addresses]` - address aliases (e.g., `@me` will be treated as a `0x0` address)

```toml
{{#include ../../examples/Move.toml.example}}
```

## Init function

Init function is a special function that gets executed only once - when the associated module is published. It always has the same signature and only
one argument:
```move
fun init(ctx: &mut TxContext) { /* ... */ }
```

For example:

```move
{{#include ../../examples/sources/basics/init-function.move:4:}}
```


## Entry functions

An [entry function](../build/move/index.md#entry-functions) visibility modifier allows a function to be called directly (e.g., in transaction). It is combinable with other
visibility modifiers, such as `public` which allows calling from other modules) and `public(friend)` for calling from *friend* modules.

```move
{{#include ../../examples/sources/basics/entry-functions.move:4:}}
```


## Strings

Move does not have a native type for strings, but it has a handy wrapper!

```move
{{#include ../../examples/sources/basics/strings.move:4:}}
```


## Shared object

Shared object is an object that is shared using a `sui::transfer::share_object` function and is accessible to everyone.

```move
{{#include ../../examples/sources/basics/shared-object.move:4:}}
```


## Transfer

To make an object freely transferable, use a combination of `key` and `store` abilities.

```move
{{#include ../../examples/sources/basics/transfer.move:4:}}
```


## Custom transfer

In Sui Move, objects defined with only `key` ability can not be transferred by default. To enable
transfers, publisher has to create a custom transfer function. This function can include any arguments,
for example a fee, that users have to pay to transfer.

```move
{{#include ../../examples/sources/basics/custom-transfer.move:4:}}
```


## Events

Events are the main way to track actions on chain.

```move
{{#include ../../examples/sources/basics/events.move:4:}}
```


## One time witness

One Time Witness (OTW) is a special instance of a type which is created only in the module initializer and is guaranteed to be unique and have only one instance. It is important for cases where we need to make sure that a witness-authorized action was performed only once (for example - [creating a new Coin](../explore/move-examples/samples.md#coin)). In Sui Move a type is considered an OTW if its definition has the following properties:

- Named after the module but uppercased
- Has only `drop` ability

> To check whether an instance is an OTW, [`sui::types::is_one_time_witness(witness)`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/types.move) should be used.

To get an instance of this type, you need to add it as the first argument to the `init()` function: Sui runtime supplies both initializer arguments automatically.

```move
module examples::mycoin {

    /// Name matches the module name
    struct MYCOIN has drop {}

    /// The instance is received as the first argument
    fun init(witness: MYCOIN, ctx: &mut TxContext) {
        /* ... */
    }
}
```

---

Example which illustrates how OTW could be used:

```move
{{#include ../../examples/sources/basics/one-time-witness.move:4:}}
```