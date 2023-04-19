---
title: Why Move?
---

In Sui, you write [Smart Contracts](../build/move/index.md) with the Sui Move Programming language. This page links to key [Move](https://golden.com/wiki/Move_(programming_language)-MNA4DZ6) resources and compares the [Move](https://github.com/move-language/move/tree/main/language/documentation) and Solidity programming languages. For a full description of the issues with traditional smart contract languages, see the [Move Problem Statement](https://github.com/MystenLabs/awesome-move/blob/main/docs/problem_statement.md).

## Sui Move

First, note Move is based upon the well-supported [Rust](https://www.rust-lang.org/) programming language. And [Sui Move differs from core Move](sui-move-diffs.md) in subtle yet distinct ways. Here are resources to ramp up on Sui Move:

 * [Sui Move announcement](https://sui.io/resources-move/why-we-created-sui-move/)
 * [Sui source code](https://github.com/MystenLabs/sui)
 * [`rustdoc` output](../build/install.md#rustdoc)
 * [Sui Move by Example](https://examples.sui.io)

## Move resources

This section aggregates links to external resources on the Move programming language. See also our [Smart Contracts with Move](../build/move/index.md) page and [Move Programming with Objects](../build/programming-with-objects/index.md) tutorial series for key Move resources in this site.

 * [Move & Sui podcast](https://zeroknowledge.fm/228-2/) on Zero Knowledge where programmable objects are described in detail.
 * Original [Move Book](https://move-book.com/index.html) written by a member of the Sui team.
 * [Core Move](https://github.com/move-language/move/tree/main/language/documentation) documentation, including:
 * [Tutorial](https://github.com/move-language/move/blob/main/language/documentation/tutorial/README.md) - A step-by-step guide through writing a Move module.
 * [Book](https://github.com/move-language/move/blob/main/language/documentation/book/src/introduction.md) - A summary with pages on [various topics](https://github.com/move-language/move/tree/main/language/documentation/book/src).
 * [Examples](https://github.com/move-language/move/tree/main/language/documentation/examples/experimental) - A set of samples, such as for [defining a coin](https://github.com/move-language/move/tree/main/language/documentation/examples/experimental/basic-coin) and [swapping it](https://github.com/move-language/move/tree/main/language/documentation/examples/experimental/coin-swap).
 * [Awesome Move](https://github.com/MystenLabs/awesome-move/blob/main/README.md) - A summary of resources related to Move, from blockchains through code samples.

## Move vs. Solidity

Currently, the main player on the blockchain languages scene is Solidity. As one of the first blockchain languages, Solidity was designed to implement basic programming language concepts using well known data types (e.g. byte array, string) and data structures (such as hashmaps) with the ability to build custom abstractions using a well-known base.

However, as blockchain technology developed it became clear that the main purpose of blockchain languages is operating on digital assets, and the main quality of such languages is security and verifiability (which is an additional layer of security). 

Move was specifically designed to address both problems: representation of digital assets and safe operations over them. To provide additional protection, it has been co-developed along with the [Move Prover](https://arxiv.org/abs/2110.08362) verification tool. This allows Move developers to write formal specifications for the key correctness properties of their application, then use the prover to check that these properties will hold for all possible transactions and inputs.

One fundamental difference between the EVM and Move is the data model for assets:
 * EVM assets are encoded as entries in `owner_address -> <bytes encoding asset>` hash maps. Asset updates and transfers work by updating entries in this map. There is no type or value representing an asset, and thus an asset cannot be passed as an argument, returned from a function, or be stored inside of another asset. Only unstructured bytes can be passed across contract boundaries, and thus each asset is forever trapped inside the contract that defines it.
 * Move assets are arbitrary user-defined types. Assets can be passed as arguments, returned from functions, and stored inside other assets. In addition, assets can flow freely across contract boundaries without losing their integrity thanks to Move's built-in *resource safety* [1](https://diem-developers-components.netlify.app/papers/diem-move-a-language-with-programmable-resources/2020-05-26.pdf) [2](https://arxiv.org/abs/2004.05106) protections.

Sui heavily leverages the Move data model for performance. Sui's persistent state is a set of programmable Move objects that can be updated, created, and destroyed by transactions. Each object has ownership metadata that allows Sui validators to both execute and commit transactions using the object in parallel with causally unrelated transactions. Move's type system ensures the integrity of this ownership metadata across executions. The result is a system where developers write ordinary Move smart contracts, but validators leverage the data model to execute and commit transactions as efficiently as possible.

This is simply not possible with the EVM data model. Because assets are stored in dynamically indexable maps, a validator would be unable to determine when transactions might touch the same asset. Sui's parallel execution and commitment scheme needs a language like Move with the vocabulary to describe structured assets that can flow freely across contracts. To be blunt: **even if we preferred the EVM/Solidity to Move, we could not use them in Sui without sacrificing the performance breakthroughs that make Sui unique**.

One of the main advantages of Move is data composability. It is always possible to create a new struct (asset) Y that will hold initial asset X in it. Even more - with addition of generics, it is possible to define generic wrapper Z(T) that will be able to wrap any asset, providing additional properties to a wrapped asset or combining it with others. See how composability works in our [Sandwich example](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/basics/sources/sandwich.move).
