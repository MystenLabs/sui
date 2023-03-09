---
title: Package Upgrades
---

Sui smart contracts are represented by immutable package objects consisting of a collection of Move modules. Because the packages are immutable, transactions can safely access smart contracts without full consensus (fast-path transactions). If someone could change these packages, they would become [shared objects](../learn/objects.md#shared), which would require full consensus before completing a transaction. 

The inability to change package objects, however, becomes a problem when considering the iterative nature of code development. Smart contract developers require the ability to update their code and pull changes from other developers while still being able to reap the benefits of fast-path transactions. Fortunately, the Sui network provides a method of upgrading your packages while still retaining their immutable properties.   

## Requirements

To upgrade a package, your package must satisfy the following requirements:
* You must be the original publisher of the package you want to upgrade.
* Your changes must be layout-compatible with the previous version. 
    * Existing `public` function signatures and struct layouts must remain the same.
    * You can add new structs and functions.
    * You can change function implementations.
    * You can change non-`public` function signatures, including `entry` and `friend` function signatures.

**Note:** If you have a package with a dependency, and that dependency is upgraded, your package does not automatically depend on the newer version. You must explicitly upgrade your own package to point to the new dependency.

## Upgrading

Use the `sui client upgrade` command to upgrade packages that meet the previous requirements, providing values for the the following flags:

* `--gas-budget`: The maximum number of gas units that can be expended before transaction is aborted.
* `--package`: The address of the package being upgraded.
* `--cap`: The address of the upgrade cap. You receive this address as a return from the publish command.

## Example

You develop a package named `sui_package`. Its manifest looks like the following:

```move
[package]
name = "sui_package"
version = "0.0.0"
published-at = "0xC0"

[addresses]
sui_package = "0xC0"
```

When your package is ready, you publish it:

```shell
sui client publish --gas-budget 10000
```
And receive the response:

```shell
----- Certificate ----
Transaction Hash: TransactionDigest(96t8k1LjqRz8fMyXnQ33KWdQqCiiVtWKcBpPUCMiwb2A)
Transaction Signature: [Signature(AA==@+AxjkQyYQnifi7qx9PvyIvRpajUCjZVO41uny6fxWWnCHXD6OHAXzf6XEL9jSIz7al3yItVzH5VsluMSN7OIAA==@aNxLU5gVv2cahhUeZ7Ig6IduqqFGZB/ULs8OkUoCgBo=)]
Signed Authorities Bitmap: RoaringBitmap<[1, 2, 3]>
Transaction Kind : Publish
Sender: 0xf46dd460a0dbcc5b57deac988641a5ef29c4ab3f
Gas Payment: Object ID: 0x207bdc057921a1570ad149cbc3d04a5ba0af0960, version: 0x6776, digest: o#29BsD1YrIsFfO/nLHKSdNaSd9YHHiPsbgbsxDI3E+aU=
Gas Owner: 0xf46dd460a0dbcc5b57deac988641a5ef29c4ab3f
Gas Price: 1
Gas Budget: 1000
----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0xc1c032d9fc8d6d2a96310ff13a651eed7ff65628 , Owner: Account Address ( 0xf46dd460a0dbcc5b57deac988641a5ef29c4ab3f )
  - ID: 0xd58a43cad76d0f15e38e945c650e78835f60c190 , Owner: Immutable
Mutated Objects:
  - ID: 0x207bdc057921a1570ad149cbc3d04a5ba0af0960 , Owner: Account Address ( 0xf46dd460a0dbcc5b57deac988641a5ef29c4ab3f )
```

Later, you update your code and need to upgrade your package. Your new manifest resembles the following:

```move
[package]
name = "sui_package"
version = "0.0.1"
published-at = "0xC1"

[addresses]
sui_package = "0xC0"
```

Noticed that the `published-at` value changes, but the `sui_package` address remains the same. 

It's now time to upgrade the package:

```shell
sui client upgrade --gas-budget 10000 --package 0xC0 --cap 0xCA4C
```

The console alerts you if the new package doesn't satisfy [requirements](#requirements), otherwise the package gets published.
