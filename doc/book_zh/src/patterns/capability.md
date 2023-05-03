# 权限凭证（Capability）

权限凭证（capability）是一种与特定对象绑定授权的模式，最常见的便是`TreasuryCap`（定义在[sui::coin](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/coin.move#L19)中）。

<details>
<summary>English Version</summary>

Capability is a pattern that allows *authorizing* actions with an object. One of the most common capabilities is `TreasuryCap` (defined in [sui::coin](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/coin.move#L19)).

</details>

```move
{{#include ../../examples_zh/sources/patterns/capability.move:4:}}
```


