# Sui by Example

The documentation elsewhere on this site examines the [Sui Move](https://docs.sui.io/learn/sui-move-diffs) dialect of the [Move](https://docs.sui.io/learn/why-move) programming language. The site explains the fundamentals of [programming with objects](https://docs.sui.io/build/programming-with-objects) and details how to use Sui Move to [write smart contracts](https://docs.sui.io/build/move), offering [smart contract examples](https://docs.sui.io/explore/examples) that apply these concepts to real-world use cases.

The content in this section expands upon the smart contract examples, providing a component-by-component breakdown for reference when creating your own modules. What is more expressive in the world of code than the code itself? In this section, you'll find examples for most of the features of Sui Move, as well as a number of advanced patterns you can leverage right away to improve your modules.

All code samples are written with the assumption that you use *Sui Move*, which you can install with this command:
```
$ cargo install --locked --git https://github.com/MystenLabs/sui.git --branch "main" sui
```

Keep in mind that the branch is set to `main`. If you're developing with our [devnet](https://docs.sui.io/build/devnet) or need more details, follow the instructions at [install Sui](https://docs.sui.io/build/install#binaries).