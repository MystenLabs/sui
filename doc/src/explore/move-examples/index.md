---
title: Sui Move by Example
---

The documentation elsewhere on this site examines the [Sui Move](../../learn/sui-move-diffs.md) dialect of the [Move](../../learn/why-move.md) programming language. The site explains the fundamentals of [programming with objects](../../build/programming-with-objects/index.md) and details how to use Sui Move to [write smart contracts](../../build/move/index.md), offering [smart contract examples](../../explore/examples.md) that apply these concepts to real-world use cases.

The content in this section expands upon the smart contract examples, providing a component-by-component breakdown for reference when creating your own modules. What is more expressive in the world of code than the code itself? In this section, you'll find examples for most of the features of Sui Move, as well as a number of advanced patterns you can leverage right away to improve your modules.

All code samples are written with the assumption that you use *Sui Move*, which you can install with this command:
```
$ cargo install --locked --git https://github.com/MystenLabs/sui.git --branch "main" sui
```

Keep in mind that the branch is set to `main`. If you're developing with [devnet](../../build/devnet.md) or need more details, follow the instructions at [install Sui](../../build/install.md#install-or-update-sui-binaries).