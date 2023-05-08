# 发布者（Publisher）

发布者（publisher）对象用于代表发布者的权限。这个对象本身并不表示任何特定的用例，它主要运用于只有两个函数：`package::from_module<T>` 和 `package::from_package<T>`，允许检查类型 T 是否属于由“Publisher”对象创建的模块或包。

<details>
<summary>English Version</summary>

Publisher Object serves as a way to represent the publisher authority. The object itself does not imply any specific use case and has only two main functions: `package::from_module<T>` and `package::from_package<T>` which allow checking whether a type `T` belongs to a module or a package for which the `Publisher` object was created.

</details>

我们强烈建议为大多数定义新对象的包创建“Publisher”对象 - 这是设置“显示”以及允许该类型在“Kiosk”生态系统中进行交易的前提条件。

> 尽管 `Publisher` 本身是一种实用工具, 但是它实现了所有权证明（"proof of ownership"）的功能 例如在对象显示（[Object Display](./display.md)）扮演了重要的角色.

<details>
<summary>English Version</summary>

We strongly advise to issue the `Publisher` object for most of the packages that define new Objects - it is required to set the "Display" as well as to allow the type to be traded in the "Kiosk" ecosystem.

> Although `Publisher` itself is a utility, it enables the _"proof of ownership"_ functionality, for example, it is crucial for [the Object Display](./display.md).

</details>

我们需要 OTW（One-Time-Witness）来设置发布者（publisher）以确保 `Publisher` 对象只在相应的模块中初始化一次（在包中可以初始化/创建多次）, 同时创建 `publisher` 函数在发布模块的交易中调用。

<details>
<summary>English Version</summary>

To set up a Publisher, a One-Time-Witness (OTW) is required - this way we ensure the `Publisher` object is initialized only once for a specific module (but can be multiple for a package) as well as that the creation function is called in the publish transaction.

</details>

```move
{{#include ../../examples_zh/sources/basics/publisher.move:4:}}
```




