# Hardhat Project for Move-on-EVM.

This directory contains a Hardhat project for Move-on-EVM. The `contracts` directory contains Move contracts (e.g., `FortyTwo.move`) and the equivalent Solidity contracts (e.g., `FortyTwo.sol`). This directory contains the Move implementations for ERC20, ERC721 and ERC1155. The `test` directory contains test files (e.g., `FortyTwo.test.js`) to test both Move contracts and Solidity contracts. The `script` directory contains the script files to deploy the Move contracts on a network.

To use this project, the hardhat environment (https://hardhat.org/tutorial/setting-up-the-environment.html) must to be set up first. Moreover, [`hardhat-move`](../hardhat-move/README.md) needs to be installed.

To compile the contracts, use the following Hardhat command:
```
$ npx hardhat compile
```

To test, use the following command:
```
$ npx hardhat test
```
At the end of the tests, a gas report will be generated.

You can deploy the Move contracts a network once after you properly key values in `hardhat.config.js`. To deploy ERC721 on the rinkeby Ethereum testnet (for example), use the following command:
```
$ npx hardhat run scripts/deploy_ERC721.js --network rinkeby
```
