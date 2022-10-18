# ID Pointer

ID Pointer is a technique that separates the main data (an object) and its accessors / capabilities by linking the latter to the original. There's a few different directions in which this pattern can be used:

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
