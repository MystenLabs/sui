---
title: Why Move?
---

This page compares the Move and Solidity programming languages For a full description of the issues with traditional smart contract languages, see the [Move Problem Statement](https://github.com/MystenLabs/awesome-move/blob/main/docs/problem_statement.md).

Currently, the main player on the blockchain languages scene is Solidity. As one of the first blockchain languages, Solidity was designed to implement basic programming language concepts using well known data types (e.g. byte array, string) and data structures (such as hashmaps) with the ability to build custom abstractions using a well-known base.

However, as blockchain technology developed it became clear that the main purpose of blockchain languages is operations with digital assets, and the main quality of such languages is security and verifiability (which is an additional layer of security). 

Move was specifically designed to address both problems: representation of digital assets and safe operations over them, it was also created to be verifiable. This adds a few complications and restrictions to the language (such as an inability to have dynamic dispatch) but provides the required properties to be verifiable.

Unlike Solidity where digital assets are implemented as abstractions over hashmaps (basic ERC20 token implementation is a key-value storage - address X owns Y amount of Z and has a set of methods such as `transfer()` or `balanceOf()`), Move introduces a new approach to owning and transferring assets. Each asset is represented as an ownable data structure [protected by the Move type system](https://diem-developers-components.netlify.app/papers/diem-move-a-language-with-programmable-resources/2020-05-26.pdf); the ownership model is implemented differently - instead of having a managed owner database, ownership of assets in Move is guaranteed by the VM. In other words, if an account holds an asset X, nobody will be able to take this asset by replacing an ownership record, because the asset is literally owned by that account.

One of the main advantages of Move is data composability. It is always possible to create a new struct (asset) Y that will hold initial asset X in it. Even more - with addition of generics, it is possible to define generic wrapper Z(T) that will be able to wrap any asset, providing additional properties to a wrapped asset or combining it with others. See our [Sandwich example](../../../sui_programmability/examples/basics/sources/Sandwich.move) to see how composability works.
