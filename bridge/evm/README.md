# 🏄‍♂️ Quick Start

This project leverages [Foundry](https://github.com/foundry-rs/foundry) to manage dependencies, contract compilation, testing, deployment, and on chain interactions via Solidity scripting.

#### Environment configuration

Duplicate rename the `.env.example` file to `.env`. You'll need accounts and api keys for **Infura** and **Etherscan** as well as the necessary RPC URLs. Be sure to add the required values in your newly created `.env` file.

> **Note**
> The OZ foundry upgrades library uses node to verify upgrade safety. Make sure you have node version 18.17 or higher as well as npm version 10.4 or higher installed.

#### Dependencies

To install the project dependencies, run: 

```bash
forge install https://github.com/OpenZeppelin/openzeppelin-contracts-upgradeable@v5.0.1 https://github.com/foundry-rs/forge-std@v1.3.0 https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades --no-git --no-commit
```

#### Compilation

To compile your contracts, run:

```bash
forge compile
```

#### Testing

```bash
forge clean
forge test --ffi
```

#### Coverage

```bash
forge coverage
```

#### Deployment

> **Note**
> Make sure the deployment config file for the target chain is created in the `deploy_configs` folder.
> The file should be named `<chainID>.json` and should have the same fields and in the same order (alphabetical) as the `example.json`.

```bash
forge clean
forge script script/deploy_bridge.s.sol --rpc-url <<alias>> --broadcast --verify --ffi
```

**Local deployment**

```bash
forge clean
forge script script/deploy_bridge.s.sol --fork-url anvil --broadcast --ffi
```

All deployments are saved in the `broadcast` directory.

#### External Resources

- [Writing OpenZeppelin Upgrades with Foundry](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades?tab=readme-ov-file)
- [OpenZeppelin Upgrade Requirements](https://docs.openzeppelin.com/upgrades-plugins/1.x/api-core#define-reference-contracts)
