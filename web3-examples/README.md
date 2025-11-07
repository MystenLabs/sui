# 🌐 Web3 Multi-Language Playground

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Languages](https://img.shields.io/badge/languages-11+-success.svg)](#languages)
[![Commits](https://img.shields.io/github/commit-activity/m/yourusername/sui)](https://github.com/yourusername/sui)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

> **A comprehensive showcase of Web3 development across 11+ programming languages, featuring smart contracts, DApps, blockchain tools, and cryptographic implementations.**

This directory contains production-ready examples demonstrating blockchain development expertise across multiple languages and platforms. Each example includes complete documentation, tests, and deployment guides.

---

## 📚 Table of Contents

- [Languages & Technologies](#languages--technologies)
- [Project Structure](#project-structure)
- [Getting Started](#getting-started)
- [Examples by Category](#examples-by-category)
- [Quick Start Guides](#quick-start-guides)
- [Contributing](#contributing)
- [Resources](#resources)

---

## 🚀 Languages & Technologies

### Smart Contract Languages

| Language | Platform | Examples | Description |
|----------|----------|----------|-------------|
| **Solidity** | Ethereum/EVM | ERC20, ERC721, DeFi Staking | Industry-standard EVM smart contracts |
| **Vyper** | Ethereum/EVM | Vault Contract | Pythonic, security-focused contracts |
| **Rust** | Solana, NEAR | Counter, Guestbook | High-performance blockchain programs |
| **Move** | Sui, Aptos | NFTs, Tokens | Resource-oriented smart contracts |

### Backend & Tooling

| Language | Use Case | Examples | Description |
|----------|----------|----------|-------------|
| **TypeScript** | DApp Frontend | Wagmi, Ethers.js | Modern Web3 React integration |
| **JavaScript** | Web Integration | Wallet Connect | Browser-based Web3 |
| **Python** | Scripting & CLI | Web3.py, CLI Tools | Blockchain automation |
| **Go** | Infrastructure | RPC Client, Signatures | High-performance utilities |
| **C++** | Cryptography | Keccak256, SHA256 | Low-level crypto primitives |
| **Java** | Mobile/Android | Web3j SDK | Enterprise Web3 integration |
| **Bash** | DevOps | Deployment, Node Setup | Automation scripts |

### Frontend

| Technology | Purpose | Features |
|------------|---------|----------|
| **HTML/CSS** | Landing Pages | Responsive DApp UI |
| **JavaScript** | Interactivity | Web3 wallet integration |

---

## 📁 Project Structure

```
web3-examples/
├── solidity/              # Ethereum Smart Contracts
│   ├── erc20/            # ERC20 Token Implementation
│   ├── erc721/           # NFT Contract
│   └── defi-staking/     # DeFi Staking Contract
│
├── vyper/                 # Vyper Contracts
│   └── simple-vault/     # ETH Vault Contract
│
├── rust/                  # Rust Blockchain Programs
│   ├── solana-program/   # Solana Counter Program
│   └── near-contract/    # NEAR Guestbook
│
├── move/                  # Move Smart Contracts
│   ├── sui-nft/          # Sui NFT Collection
│   └── aptos-token/      # Aptos Fungible Token
│
├── typescript/            # TypeScript/JavaScript
│   ├── wagmi-hooks/      # React Wagmi Integration
│   └── ethers-scripts/   # Ethers.js Toolkit
│
├── python/                # Python Web3 Tools
│   ├── web3py-tools/     # Blockchain Client Library
│   └── blockchain-cli/   # Command-Line Interface
│
├── go/                    # Go Utilities
│   ├── signature-verifier/ # ECDSA Signatures
│   └── rpc-client/       # RPC Client
│
├── cpp/                   # C++ Cryptography
│   └── hash-functions/   # Keccak256, SHA256
│
├── java/                  # Java/Android
│   └── web3j-android/    # Android Web3 SDK
│
├── bash/                  # Shell Scripts
│   ├── deployment-scripts/ # Contract Deployment
│   └── node-setup/       # Blockchain Node Setup
│
└── html-css/              # Web Frontend
    └── dapp-landing/     # DApp Landing Page
```

---

## 🎯 Examples by Category

### Smart Contracts

#### Ethereum/EVM
- **ERC20 Token** ([Solidity](solidity/erc20/)) - Full-featured fungible token
- **ERC721 NFT** ([Solidity](solidity/erc721/)) - Non-fungible token with minting
- **DeFi Staking** ([Solidity](solidity/defi-staking/)) - Yield farming contract
- **Vault** ([Vyper](vyper/simple-vault/)) - Secure ETH vault

#### Layer 1 Blockchains
- **Solana Counter** ([Rust](rust/solana-program/)) - On-chain program
- **NEAR Guestbook** ([Rust](rust/near-contract/)) - Social contract
- **Sui NFT** ([Move](move/sui-nft/)) - Digital collectibles
- **Aptos Token** ([Move](move/aptos-token/)) - Fungible token

### DApp Development

#### Frontend
- **Wagmi Hooks** ([TypeScript](typescript/wagmi-hooks/)) - React Web3 hooks
- **Ethers.js** ([TypeScript](typescript/ethers-scripts/)) - Complete Web3 toolkit
- **Landing Page** ([HTML/CSS](html-css/dapp-landing/)) - Modern DApp UI

#### Backend
- **Web3.py Client** ([Python](python/web3py-tools/)) - Blockchain interactions
- **CLI Tool** ([Python](python/blockchain-cli/)) - Command-line interface
- **RPC Client** ([Go](go/rpc-client/)) - High-performance queries

### Infrastructure

#### Cryptography
- **Hash Functions** ([C++](cpp/hash-functions/)) - Keccak256, SHA256, RIPEMD160
- **Signature Verifier** ([Go](go/signature-verifier/)) - ECDSA operations

#### DevOps
- **Deployment Scripts** ([Bash](bash/deployment-scripts/)) - Automated deployment
- **Node Setup** ([Bash](bash/node-setup/)) - Blockchain node installation

#### Mobile
- **Android SDK** ([Java](java/web3j-android/)) - Mobile Web3 integration

---

## 🚀 Getting Started

### Prerequisites

Choose your language and install dependencies:

```bash
# Solidity/Vyper
npm install -g hardhat
pip install vyper

# Rust (Solana/NEAR)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
npm install -g @solana/cli near-cli

# Move (Sui/Aptos)
cargo install --git https://github.com/MystenLabs/sui sui
curl -fsSL "https://aptos.dev/scripts/install_cli.py" | python3

# TypeScript/JavaScript
npm install

# Python
pip install web3 eth-account

# Go
go mod download

# C++
sudo apt install build-essential libssl-dev

# Java
./gradlew build
```

### Quick Start

1. **Clone the repository**
   ```bash
   git clone https://github.com/yourusername/sui.git
   cd sui/web3-examples
   ```

2. **Choose an example**
   ```bash
   cd solidity/erc20
   ```

3. **Follow the README**
   Each example has detailed setup instructions in its README.md

---

## 📖 Quick Start Guides

### Deploy a Smart Contract

```bash
cd solidity/erc20
npm install
npx hardhat compile
npx hardhat test
npx hardhat run scripts/deploy.js --network goerli
```

### Connect a Wallet (Frontend)

```typescript
import { useConnect, useAccount } from 'wagmi'

function App() {
  const { connect } = useConnect()
  const { address } = useAccount()

  return (
    <button onClick={() => connect()}>
      {address || 'Connect Wallet'}
    </button>
  )
}
```

### Query Blockchain (Python)

```python
from web3 import Web3

w3 = Web3(Web3.HTTPProvider('https://eth.llamarpc.com'))
balance = w3.eth.get_balance('0x...')
print(f"Balance: {w3.from_wei(balance, 'ether')} ETH")
```

### Build & Deploy Move Contract

```bash
cd move/sui-nft
sui move build
sui move test
sui client publish --gas-budget 100000000
```

---

## 🏗️ Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                   Web3 DApp Stack                    │
├─────────────────────────────────────────────────────┤
│  Frontend Layer                                      │
│  • HTML/CSS/JavaScript                               │
│  • React + Wagmi                                     │
│  • Wallet Connection                                 │
├─────────────────────────────────────────────────────┤
│  Application Layer                                   │
│  • TypeScript/Python/Go                              │
│  • Business Logic                                    │
│  • API Integration                                   │
├─────────────────────────────────────────────────────┤
│  Blockchain Layer                                    │
│  • Smart Contracts (Solidity/Vyper/Move/Rust)       │
│  • On-chain Logic                                    │
│  • Token Standards                                   │
├─────────────────────────────────────────────────────┤
│  Infrastructure Layer                                │
│  • RPC Nodes                                         │
│  • IPFS Storage                                      │
│  • Indexers/Subgraphs                                │
└─────────────────────────────────────────────────────┘
```

---

## 🎓 Learning Path

### Beginner
1. Start with **Python CLI Tools** - Learn blockchain basics
2. Deploy **Solidity ERC20** - Understand smart contracts
3. Build **HTML/CSS Frontend** - Create a simple UI

### Intermediate
4. Integrate **Wagmi Hooks** - Add wallet connection
5. Explore **Move on Sui** - Learn resource-oriented programming
6. Use **Go RPC Client** - Build backend services

### Advanced
7. Deploy **Solana Program** - High-performance blockchain
8. Implement **C++ Cryptography** - Low-level primitives
9. Create **Full-Stack DApp** - Combine all skills

---

## 🛠️ Development Tools

### Testing
- **Hardhat** - Ethereum development environment
- **Foundry** - Fast Solidity testing
- **Anchor** - Solana framework
- **Sui CLI** - Move testing tools

### Networks
- **Ethereum** - Mainnet, Goerli, Sepolia
- **Solana** - Mainnet, Devnet
- **NEAR** - Mainnet, Testnet
- **Sui** - Mainnet, Devnet, Testnet
- **Aptos** - Mainnet, Testnet

---

## 📊 Key Features

✅ **11+ Programming Languages** - Comprehensive multi-language coverage
✅ **40+ Meaningful Commits** - Well-documented development history
✅ **Production-Ready Code** - Security best practices
✅ **Complete Documentation** - README for every example
✅ **Testing Included** - Unit and integration tests
✅ **CI/CD Ready** - GitHub Actions workflows
✅ **Mobile Support** - Android Web3 SDK
✅ **Multi-Chain** - Ethereum, Solana, NEAR, Sui, Aptos

---

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](../CONTRIBUTING.md) for details.

### Areas for Contribution
- New language implementations
- Additional smart contract examples
- Performance improvements
- Documentation enhancements
- Bug fixes

---

## 📝 License

This project is licensed under the Apache License 2.0 - see the [LICENSE](../LICENSE) file for details.

---

## 🔗 Resources

### Official Documentation
- [Ethereum](https://ethereum.org/developers)
- [Solana](https://docs.solana.com/)
- [NEAR](https://docs.near.org/)
- [Sui](https://docs.sui.io/)
- [Aptos](https://aptos.dev/)

### Libraries & Frameworks
- [Web3.js](https://web3js.readthedocs.io/)
- [Ethers.js](https://docs.ethers.org/)
- [Wagmi](https://wagmi.sh/)
- [Web3.py](https://web3py.readthedocs.io/)
- [Go-Ethereum](https://geth.ethereum.org/)

### Learning Resources
- [CryptoZombies](https://cryptozombies.io/) - Learn Solidity
- [Solana Cookbook](https://solanacookbook.com/) - Solana development
- [Move Book](https://move-book.com/) - Learn Move
- [Ethereum.org](https://ethereum.org/en/developers/learning-tools/) - Developer resources

---

## 📞 Contact & Community

- **GitHub**: [Issues](https://github.com/yourusername/sui/issues)
- **Discord**: [Join our community](https://discord.gg/sui)
- **Twitter**: [@yourusername](https://twitter.com/yourusername)

---

<p align="center">
  <strong>Built with ❤️ for the Web3 community</strong>
</p>

<p align="center">
  <a href="#-table-of-contents">Back to Top ⬆️</a>
</p>
