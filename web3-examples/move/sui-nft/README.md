# Sui NFT Digital Collectible

A complete NFT implementation on the Sui blockchain using the Move programming language.

## Features

- ✅ Mint NFTs with metadata (name, description, URL)
- ✅ Transfer NFTs to other addresses
- ✅ Update description (creator only)
- ✅ Burn/delete NFTs
- ✅ Event emission for tracking
- ✅ Built-in unit tests

## About Move on Sui

Sui uses Move, a resource-oriented programming language that provides:
- **Object-centric**: Every asset is a unique object
- **Safety**: Ownership and move semantics prevent common bugs
- **Flexibility**: Parallel transaction execution
- **Speed**: Sub-second finality

## Setup

Install Sui:
```bash
cargo install --locked --git https://github.com/MystenLabs/sui.git --branch mainnet sui
```

## Build

```bash
sui move build
```

## Test

```bash
sui move test
```

## Deploy

```bash
# Switch to devnet
sui client switch --env devnet

# Get test SUI
sui client faucet

# Publish package
sui client publish --gas-budget 100000000
```

## Usage

### Mint NFT

```bash
sui client call \
  --package PACKAGE_ID \
  --module digital_collectible \
  --function mint_nft \
  --args "My NFT" "A beautiful collectible" "https://example.com/nft.png" \
  --gas-budget 10000000
```

### Transfer NFT

```bash
sui client call \
  --package PACKAGE_ID \
  --module digital_collectible \
  --function transfer_nft \
  --args NFT_OBJECT_ID RECIPIENT_ADDRESS \
  --gas-budget 10000000
```

### Update Description

```bash
sui client call \
  --package PACKAGE_ID \
  --module digital_collectible \
  --function update_description \
  --args NFT_OBJECT_ID "New description" \
  --gas-budget 10000000
```

### Burn NFT

```bash
sui client call \
  --package PACKAGE_ID \
  --module digital_collectible \
  --function burn_nft \
  --args NFT_OBJECT_ID \
  --gas-budget 10000000
```

## Module Structure

```move
module sui_nft::digital_collectible {
    struct DigitalCollectible {
        id: UID,
        name: String,
        description: String,
        url: String,
        creator: address,
    }

    // Functions
    - mint_nft()
    - transfer_nft()
    - update_description()
    - burn_nft()
}
```

## Events

- `NFTMinted` - Emitted when NFT is created
- `NFTTransferred` - Emitted when NFT is transferred

## Security Features

- ✅ Creator verification for updates
- ✅ Proper ownership transfer
- ✅ Safe object deletion
- ✅ Move's built-in safety guarantees

## Why Sui?

- **Fast**: Sub-second finality
- **Cheap**: Low transaction costs
- **Scalable**: Parallel execution
- **Developer-Friendly**: Modern tooling

## Resources

- [Sui Documentation](https://docs.sui.io/)
- [Move Book](https://move-book.com/)
- [Sui Examples](https://github.com/MystenLabs/sui/tree/main/examples)
