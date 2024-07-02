# 创建 ERC20 代币（Create a Coin (ERC20)）

在 Sui 上发布一种代币几乎和发布一个新类型一样简单。但是，不同的是，它需要使用一次性见证（[One Time Witness](/basics/one-time-witness.md)）。

<details>
<summary>English Version</summary>

Publishing a coin is Sui is almost as simple as publishing a new type. However it is a bit tricky as it requires using a [One Time Witness](/basics/one-time-witness.md).

</details>

```move
{{#include ../../examples_zh/sources/samples/coin.move:4:}}
```

`Coin<T>` 是 Sui 上代币的通用实现。`TreasuryCap` 的所有者可以控制硬币的铸造和销毁，通过与 `sui::coin::Coin` 交互，并使用 `TreasuryCap` 对象作为授权。

<details>
<summary>English Version</summary>

The `Coin<T>` is a generic implementation of a Coin on Sui. Owner of the `TreasuryCap` gets control over the minting and burning of coins. Further transactions can be sent directly to the `sui::coin::Coin` with `TreasuryCap` object as authorization.

</details>