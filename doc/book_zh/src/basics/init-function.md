# 初始化函数（Init Function）

初始化函数（`init`）是一个只在模块发布时被执行一次的函数，函数签名总是一致和且只有一个参数：
```move
fun init(ctx: &mut TxContext) { /* ... */ }
```

<details>
<summary>English Version</summary>

Init function is a special function that gets executed only once - when the associated module is published. It always has the same signature and only one argument:
```move
fun init(ctx: &mut TxContext) { /* ... */ }
```

</details>


示例:

```move
{{#include ../../examples_zh/sources/basics/init-function.move:4:}}
```
