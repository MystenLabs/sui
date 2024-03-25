# 烫手山芋（Hot Potato）

烫手山芋（Hot Potato）是一个没有任何能力修饰符的结构体，因此它只能在其模块中被打包和解包。如果函数 A 返回这样一个结构，而函数 B 消耗它，那么我们必须在函数 A 之后调用函数 B。

<details>
<summary>English Version</summary>

Hot Potato is a name for a struct that has no abilities, hence it can only be packed and unpacked in its module. In this struct, you must call function B after function A in the case where function A returns a potato and function B consumes it.

</details>

```move
{{#include ../../examples_zh/sources/patterns/hot_potato.move:4:}}
```

烫手山芋模式被应用于以下例子中:

- [Flash Loan](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/flash_lender.move)