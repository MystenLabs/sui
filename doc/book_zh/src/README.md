# Sui by Example（中文版）

欢迎来到[docs.sui.io](https://docs.sui.io/)的配套书籍。我们在[Sui Move](https://docs.sui.io/learn/sui-move-diffs)中介绍了Sui Move是[Move](https://docs.sui.io/learn/why-move)语言的变体， 同时解释了如何使用 Sui Move 进行[智能合约开发](https://docs.sui.io/build/move)以及[面向对象编程](https://docs.sui.io/build/programming-with-objects)。

<details>
<summary>English Version</summary>

Welcome to the companion book to [docs.sui.io](https://docs.sui.io/). There we describe the [Sui Move](https://docs.sui.io/learn/sui-move-diffs) variant of the [Move](https://docs.sui.io/learn/why-move) programming language and explain how to use it to [write smart contracts](https://docs.sui.io/build/move) and [programming with objects](https://docs.sui.io/build/programming-with-objects).

</details>

不同的是，在这本书中将基于[智能合约示例](https://docs.sui.io/explore/examples)来解释不同模块、概念的使用, 方便读者随时参考。毕竟在代码的世界中还有什么比代码本身更具表现力的呢！在这本书中你将看到 Sui Move 的大部分特性以及一些可以直接使用的高级设计模式以提升您的模块。

<details>
<summary>English Version</summary>

Instead, this site builds upon the [smart contract examples](https://docs.sui.io/explore/examples) already highlighted with component-by-component examples you may reference at any time. What is more expressive in the world of code than the code itself? In this book, you'll find examples for most of the features of Sui Move as well as a number of advanced patterns that can be used right away to improve your modules.

</details>

本书中所有示例都基于 *Sui Move* 开发， 您可以通过以下命令安装 *Sui Move*：
**！！！请注意书中代码示例中所有的中文注释仅为翻译需要，实际开发中 move 语言暂不支持 UTF-8 编码注释。**

```
$ cargo install --locked --git https://github.com/MystenLabs/sui.git --branch "main" sui
```

值得注意的是，上面的命令设置的分支为 `main` 主分支，如果在 [`devnet`](https://docs.sui.io/build/devnet) 网络开发请参考 [install Sui](https://docs.sui.io/build/install#install-sui-binaries)。

<details>
<summary>English Version</summary>

All code samples in this book are written with the assumption that you use *Sui Move*, which can installed with this command:
```
$ cargo install --locked --git https://github.com/MystenLabs/sui.git --branch "main" sui
```

Keep in mind that the branch is set to `main`. If you're developing with our [devnet](https://docs.sui.io/build/devnet), instead follow the instructions to [install Sui](https://docs.sui.io/build/install#install-sui-binaries).

</details>