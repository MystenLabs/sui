# 一次性见证（One Time Witness）

一次性见证（One Time Witness, OTW）是一种特殊类型的实例，仅在模块初始化器中创建，并保证是唯一的，并且只有一个实例。在需要确保仅执行一次见证授权操作的情况下非常重要（例如-[创建新的Coin](/samples/coin.md)）。在 Sui Move中，如果一个类型的定义具有以下属性，则被视为 OTW：

- 以模块的名字命名，但是所有字母大写
- 只拥有 `drop` 修饰符

> 可以使用[`sui::types::is_one_time_witness(witness)`](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/types.move)来检查一个实例是不是OTW。

<details>
<summary>English Version</summary>

One Time Witness (OTW) is a special instance of a type which is created only in the module initializer and is guaranteed to be unique and have only one instance. It is important for cases where we need to make sure that a witness-authorized action was performed only once (for example - [creating a new Coin](/samples/coin.md)). In Sui Move a type is considered an OTW if its definition has the following properties:

- Named after the module but uppercased
- Has only `drop` ability

>To check whether an instance is an OTW, [`sui::types::is_one_time_witness(witness)`](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/types.move) should be used.

</details>

为了得到这个类型的实例，我们需要把它作为第一个参数传入到 visibility`init()` 函数： Sui 运行时自动提供两个初始化参数。

```move
module examples::mycoin {

    /// 以模块的名字命名
    /// Name matches the module name
    struct MYCOIN has drop {}

    /// 将OTW作为第一个参数传入`init` 函数
    /// The instance is received as the first argument
    fun init(witness: MYCOIN, ctx: &mut TxContext) {
        /* ... */
    }
}
```
<details>
<summary>English Version</summary>

To get an instance of this type, you need to add it as the first argument to the `init()` function: Sui runtime supplies both initializer arguments automatically.

```move
module examples::mycoin {

    /// Name matches the module name
    struct MYCOIN has drop {}

    /// The instance is received as the first argument
    fun init(witness: MYCOIN, ctx: &mut TxContext) {
        /* ... */
    }
}
```

</details>

---
通过以下例子我们可以更好地了解如何使用OTW:
```move
{{#include ../../examples_zh/sources/basics/one-time-witness.move:4:}}
```

---










