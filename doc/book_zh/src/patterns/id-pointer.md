# ID 指针（ID Pointer）

ID指针模式旨在将主要数据（一个对象）与其访问器/权限分离。这种模式可以用于几个不同的方向：
- 为共享对象提供可转让（转移）的功能（例如，利用 `TransferCap` 权限更改共享对象的 “owner” 字段）
- 将动态数据与静态数据分开（例如，单个 NFT 及其作品集的信息）
- 避免不必要的类型链接（以及见证的要求）（例如，流动性池中的 LP 代币）


<details>
<summary>English Version</summary>

ID Pointer is a technique that separates the main data (an object) and its accessors / capabilities by linking the latter to the original. There's a few different directions in which this pattern can be used:

- issuing transferable capabilities for shared objects (for example, a TransferCap that changes 'owner' field of a shared object)
- splitting dynamic data and static (for example, an NFT and its Collection information)
- avoiding unnecessary type linking (and witness requirement) in generic applications (LP token for a LiquidityPool)

</details>


```move
{{#include ../../examples_zh/sources/patterns/id_pointer.move:4:}}
```

ID 指针模式被应用于以下例子中:

- [Lock](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/basics/sources/lock.move)
- [Escrow](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/escrow.move)
- [Hero](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/games/sources/hero.move)
- [Tic Tac Toe](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/games/sources/tic_tac_toe.move)
- [Auction](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/nfts/sources/auction.move)
