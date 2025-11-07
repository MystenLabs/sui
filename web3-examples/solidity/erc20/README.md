# ERC20 Token Example

A standard ERC20 token implementation using OpenZeppelin contracts.

## Features

- ✅ Standard ERC20 functionality (transfer, approve, transferFrom)
- ✅ Minting capability (owner only)
- ✅ Burning mechanism
- ✅ Maximum supply cap (1,000,000 tokens)
- ✅ OpenZeppelin security patterns

## Setup

```bash
npm install
```

## Compile

```bash
npx hardhat compile
```

## Test

```bash
npx hardhat test
```

## Deploy

```bash
npx hardhat run scripts/deploy.js --network localhost
```

## Contract Details

- **Name**: MyToken
- **Symbol**: MTK
- **Decimals**: 18
- **Initial Supply**: 100,000 MTK
- **Max Supply**: 1,000,000 MTK

## Dependencies

- Solidity ^0.8.20
- OpenZeppelin Contracts ^5.0.0
- Hardhat ^2.19.0
