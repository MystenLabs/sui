# Move-on-EVM

> NOTE: this tree contains an experimental version of Move which runs on the EVM. The programming model is
> different from regular Move. The examples in this directory do not work with the usual Move
> tools and blockchains.

"Move-on-EVM" is a programming model in Move for EVM. In the current model, *each Move EVM contract has its own isolated address space*. This reflects the setup of the EVM most naturally, where storage between contracts cannot be shared apart from via accessor contract functions. Move EVM contracts use attributes to indicate the usage of structs for storage and events, and for functions to be callable from other contracts. It is expected that there is some codegen of Move from these attributes. For example, functions marked as `callable` have a generated API for cross-contract EVM call and delegate invocations. The module [Evm.move](./stdlib/sources/Evm.move) contains the API of a Move contract to the EVM. It encapsulates access to the transaction context and other EVM builtins

This directory contains the following sub-directories:
- [move-to-yul](./move-to-yul) is a cross compiler that compiles Move contracts to the intermediate language Yul.
- [stdlib](./stdlib) is the standard library for Move-on-EVM.
- [examples](./examples) contains multiple Move-on-EVM examples including ERC20, ERC721 and ERC1155.
- [hardhat-examples](./hardhat-examples) is a Hardhat project to demonstrate Move-on-EVM.
- [hardhat-move](./hardhat-move) is a Hardhat plugin to support the Move language.
- [exec-utils](./exec-utils) contains EVM execution utils.
