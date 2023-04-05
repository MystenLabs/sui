# Move-on-EVM Examples

This directory contains (a growing set of) examples of "Move-on-EVM", a programming model in Move for EVM.

- [Token.move](./sources/Token.move) contains an implementation of ERC20 which is intended to be compliant and callable
  from other EVM code (Move or otherwise).
- [Faucet.move](./sources/Faucet.move) contains the faucet example from the Ethereum book.
- [ERC20.move](./sources/ERC20.move) contains another implementation of the ERC20 standard which uses a single `struct` to represent the contract state.
- [ERC165.move](./sources/ERC165.move) contains a sample implementation of the ERC165 standard.
- [ERC721.move](./sources/ERC721.move) contains an implementation of ERC721 which is the standard for non-fungible tokens.
- [ERC1155.move](./sources/ERC1155.move) contains an implementation of ERC1155 which is the standard for multi-tokens.
- [TestUniswap.move](./sources/TestUniswap.move) and [TestUniswapLiquidity.move](./sources/TestUniswapLiquidity.move) are the sample client modules of `Uniswap`.

This directory is a Move package. To build the source files, use `move build`. Moreover, use `move test` to run the unit tests located in the `tests` directory.
