---
title: Patterns
---

This part covers the programming patterns that are widely used in Move; some of which can exist only in Move.


## Capability

Capability is a pattern that allows *authorizing* actions with an object. One of the most common capabilities is `TreasuryCap` (defined in [sui::coin](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/coin.move#L19)).


```move
{{#include ../../examples/sources/patterns/capability.move:4:}}
```

## Witness

Witness is a pattern that is used for confirming the ownership of a type. To do so, pass a `drop` instance of a type. Coin relies on this implementation.

```move
{{#include ../../examples/sources/patterns/witness.move:4:}}
```

This pattern is used in these examples:

- [Liquidity pool](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/pool.move)
- [Regulated coin](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/fungible_tokens/sources/regulated_coin.move)


## Transferable witness

```move
{{#include ../../examples/sources/patterns/transferable-witness.move:4:}}
```


## Hot potato

Hot Potato is a name for a struct that has no abilities, hence it can only be packed and unpacked in its module. In this struct, you must call function B after function A in the case where function A returns a potato and function B consumes it.

```move
{{#include ../../examples/sources/patterns/hot_potato.move:4:}}
```

This pattern is used in these examples:

- [Flash Loan](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/flash_lender.move)


## ID pointer

ID Pointer is a technique that separates the main data (an object) and its accessors / capabilities by linking the latter to the original. There's a few different directions in which you can use this pattern:

- issuing transferable capabilities for shared objects (for example, a TransferCap that changes 'owner' field of a shared object)
- splitting dynamic data and static (for example, an NFT and its Collection information)
- avoiding unnecessary type linking (and witness requirement) in generic applications (LP token for a LiquidityPool)

```move
{{#include ../../examples/sources/patterns/id_pointer.move:4:}}
```

This pattern is used in these examples:

- [Lock](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/basics/sources/lock.move)
- [Escrow](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/escrow.move)
- [Hero](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/games/sources/hero.move)
- [Tic Tac Toe](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/games/sources/tic_tac_toe.move)
- [Auction](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/nfts/sources/auction.move)