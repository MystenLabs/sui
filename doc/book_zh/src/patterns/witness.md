# 见证（Witness）

见证（witness）模式被用于验证某一类型的所有权。为此，需要传递一个拥有 drop 的类型实例。Coin 就使用了这种模式。

<details>
<summary>English Version</summary>

Witness is a pattern that is used for confirming the ownership of a type. To do so, one passes a `drop` instance of a type. Coin relies on this implementation.

</details>

```move
{{#include ../../examples_zh/sources/patterns/witness.move:4:}}
```

见证模式被应用于以下例子中:

- [Liquidity pool](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/pool.move)
- [Regulated coin](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/fungible_tokens/sources/regulated_coin.move)
