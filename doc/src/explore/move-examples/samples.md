---
title: Samples
---

This section contains a collection of ready-to-go samples for common blockchain use cases.

## Make an NFT

In Sui, everything is an NFT - Objects are unique, non-fungible, and owned. So technically, a simple type publishing is enough.

```move
{{#include ../../examples/sources/samples/nft.move:4:}}
```


## Create a coin

Publishing a coin in Sui is similar to publishing a new type; however, it's a little more complicated as it requires using a [One Time Witness](../explore/move-examples/basics.md#one-time-witness).

```move
{{#include ../../examples/sources/samples/coin.move:4:}}
```

The `Coin<T>` is a generic implementation of a Coin on Sui. Owner of the `TreasuryCap` gets control over the minting and burning of coins. Further transactions can be sent directly to the `sui::coin::Coin` with `TreasuryCap` object as authorization.
