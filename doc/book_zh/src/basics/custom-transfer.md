# 自定义转移（Custom transfer）

在 Sui Move 中，只拥有 `key` 能力的对象的所有权是不可以被随意转移的， 想要使只有 `key` 限制符对象可以变更所有权，必须创建一个自定义的转移函数（`transfer`）。 这个函数可以包括任意的参数， 例如在转移对象时所需的费用等（如示例中）。

<details>
<summary>English Version</summary>

In Sui Move, objects defined with only `key` ability can not be transferred by default. To enable transfers, publisher has to create a custom transfer function. This function can include any arguments, for example a fee, that users have to pay to transfer.

</details>

```move
{{#include ../../examples_zh/sources/basics/custom-transfer.move:4:}}
```
