# ERC721 NFT Example

A complete ERC721 NFT implementation with minting, URI storage, and payment handling.

## Features

- ✅ ERC721 standard compliance
- ✅ Metadata URI storage
- ✅ Paid minting (0.05 ETH default)
- ✅ Max supply limit (10,000 NFTs)
- ✅ Owner withdrawal mechanism
- ✅ Dynamic price adjustment

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

## Usage

```javascript
// Mint an NFT
await myNFT.mintNFT(
  "0x...",
  "ipfs://QmXxx.../metadata.json",
  { value: ethers.parseEther("0.05") }
);

// Check total minted
const totalMinted = await myNFT.totalSupply();

// Get token URI
const uri = await myNFT.tokenURI(1);
```

## Contract Details

- **Name**: MyNFT
- **Symbol**: MNFT
- **Max Supply**: 10,000
- **Mint Price**: 0.05 ETH
- **Standard**: ERC721 + URI Storage

## Dependencies

- Solidity ^0.8.20
- OpenZeppelin Contracts ^5.0.0
