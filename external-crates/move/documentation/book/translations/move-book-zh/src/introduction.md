# 引言 (Introduction)

Welcome to Move, a next generation language for secure, sandboxed, and formally verified programming. Its first use case is for the Diem blockchain, where Move provides the foundation for its implementation. Move allows developers to write programs that flexibly manage and transfer assets, while providing the security and protections against attacks on those assets. However, Move has been developed with use cases in mind outside a blockchain context as well.

欢迎来到Move的世界，Move是一种安全、沙盒式和形式化验证的下一代编程语言，它的第一个用例是 Diem 区块链(当时名字叫Libra, 脸书团队开发的项目, 译者注), Move 为其实现提供了基础。 Move 允许开发人员编写灵活管理和转移数字资产的程序，同时提供安全保护，防止对那些链上资产的攻击。不仅如此，Move 也可用于区块链世界之外的开发场景。

Move takes its cue from [Rust](https://www.rust-lang.org/) by using resource types with move (hence the name) semantics as an explicit representation of digital assets, such as currency.

Move 的诞生从[Rust](https://www.rust-lang.org/)中吸取了灵感，Move也是因为使用具有移动(move)语义的资源类型作为数字资产(例如货币)的显式表示而得名。

## Move是为谁准备的？(Who is Move for?)

Move was designed and created as a secure, verified, yet flexible programming language. The first use of Move is for the implementation of the Diem blockchain. That said, the language is still evolving. Move has the potential to be a language for other blockchains, and even non-blockchain use cases as well.

Move语言被设计和创建为安全、可验证， 同时兼顾灵活性的编程语言。Move的第一个应用场景是用于Diem区块链的开发。现在，Move语言仍在不断发展中。Move 还有成为其他区块链，甚至非区块链用例开发语言的潜质。

Given custom Move modules will not be supported at the [launch](https://diem.com/white-paper/#whats-next) of the Diem Payment Network (DPN), we are targeting an early Move Developer persona.

鉴于在 Diem 支付网络 (DPN) [启动](https://diem.com/white-paper/#whats-next)时将不支持自定义 Move 模块(custom Move modules)，我们的目标是早期的 Move 开发人员。

The early Move Developer is one with some programming experience, who wants to begin understanding the core programming language and see examples of its usage.

早期的 Move 开发人员应该是具有一定编程经验的程序员，他们愿意了解编程语言核心，并探索它的用法。

### 爱好者 (Hobbyists)

Understanding that the capability to create custom modules on the Diem Payment Network will not be available at launch, the hobbyist Move Developer is interested in learning the intricacies of the language. She will understand the basic syntax, the standard libraries available, and write example code that can be executed using the Move CLI. The Move Developer may even want to dig into understanding how the Move Virtual Machine executes the code she writes.

作为(Move语言)爱好者角色，首先需要明白在Diem支付网络上创建自定义模块(custom modules)是不可能的，其次，你还要对探索这门语言的复杂性保持兴趣。你将了解基本语法、可用的标准库，并编写可以用的Move CLI执行的示例代码。如果可能，你甚至可以去尝试体验Move虚拟机如何执行你自己编写的代码。

### 核心贡献者 (Core Contributor)

Beyond a hobbyist wanting to stay ahead of the curve for the core programming language is someone who may want to [contribute](https://diem.com/en-US/cla-sign/) directly to Move. Whether this includes submitting language improvements or even, in the future, adding core modules available on the Diem Payment Network, the core contributor will understand Move at a deep level.

核心贡献者指那些超越爱好者并想在核心编程语言方面保持领先，还直接为 Move 做出[贡献](https://diem.com/en-US/cla-sign/)的人。无论是提交语言改进，甚至未来添加 Diem 支付网络上可用的核心模块等，核心贡献者都将深入了解Move。

### Move不适用于哪些人？(Who Move is currently not targeting)

Currently, Move is not targeting developers who wish to create custom modules and contracts for use on the Diem Payment Network. We are also not targeting novice developers who expect a completely polished developer experience even in testing the language.

目前，Move 并不适用那些希望在在 Diem 支付网络上创建自定义模块和合约的开发人员。我们也不针对期望在测试语言时就能获得完美开发体验的初学开发者。

## 从哪里开始？(Where Do I Start?)

Begin with understanding [modules and scripts](https://move-language.github.io/move/modules-and-scripts.html) and then work through the [Move Tutorial](https://move-language.github.io/move/creating-coins.html).

你可以从了解模块和脚本([modules and scripts](./modules-and-scripts.html))开始，然后跟随Move教程([Move Tutorial](./move-tutorial.html))进行练习。
