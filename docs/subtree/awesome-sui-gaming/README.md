# Awesome Sui: Gaming
_A curated list of awesome developer tools and infrastructure projects within the Sui ecosystem, for gaming._

---

<a href="https://sui.io/"><img alt="Sui logo" src="icons/Sui_Symbol_Sea.svg" align="left" width="60" /></a>
Sui is the first blockchain built for internet scale, enabling fast, scalable, and low-latency transactions. It's programmable and composable, powered by the Move language, making it easy to build and integrate dApps. Sui prioritizes developer experience and frictionless user interactions, designed to support next-gen decentralized applications with minimal complexity.

---
**[Please see the main Awesome Sui Repo here.](https://github.com/sui-foundation/awesome-sui/blob/main/README.md)**

This repo serves as a technology reference guide for game developers interested in building on the Sui blockchain. It outlines Sui's unique technical capabilities and their specific applications in gaming contexts, organized into four key areas:

1. Core architecture features like the Move language and object-centric model that enable secure, high-performance game mechanics
2. Trust and storage solutions for confidential computations and decentralized asset management
3. Asset and economy tools for creating dynamic in-game items and marketplaces
4. User experience improvements that make blockchain gaming accessible to mainstream players

The reference helps developers understand how Sui's blockchain infrastructure can address common gaming challenges while enabling new possibilities for asset ownership, game economies, and player experiences.

---

# Contents
I. [Core Architecture & Programming](https://github.com/theninthangel/awesome-sui-gaming/blob/main/README.md#i-core-architecture--programming)  
II. [Advanced Trust, Privacy & Data Storage](https://github.com/theninthangel/awesome-sui-gaming/tree/main?tab=readme-ov-file#ii-advanced-trust-privacy--data-storage)  
III.[ Asset & Economy Features](https://github.com/theninthangel/awesome-sui-gaming/tree/main?tab=readme-ov-file#iii-asset--economy-features)  
IV. [User Experience (UX) & Onboarding](https://github.com/theninthangel/awesome-sui-gaming/tree/main?tab=readme-ov-file#iv-user-experience-ux--onboarding)  
V. [Development Tooling Partnerships](https://github.com/theninthangel/awesome-sui-gaming/tree/main?tab=readme-ov-file#v-development-tooling-partnerships)  
VI. [Core Mysten Tools and SDKs](https://github.com/theninthangel/awesome-sui-gaming/tree/main?tab=readme-ov-file#vi-core-mysten-tools-and-sdks)  

---

# I. Core Architecture & Programming
| Technology | Description | Video Game Utilization |
| --- | --- | --- |
| [Sui Move Language](https://docs.sui.io/concepts/sui-move-concepts)	| A Rust-based smart contract language focusing on safety, correctness, and asset centricity, designed specifically for Sui's object model.	| Enables highly secure and bug-resistant logic for handling game-critical assets, currencies, and ownership rules. |
| [Object-Centric Model](https://docs.sui.io/concepts/object-model) | All digital assets, from currencies to player inventories to game state, are represented as independent, first class Objects with globally unique IDs, rather than being stored within contract mappings. | Allows for dynamic object updates and parallel transaction processing (speed) for uncorrelated transactions. |
| [Parallel Execution](https://blog.sui.io/parallelization-explained/) | Sui processes independent transactions (e.g., Player A mints an NFT while Player B transfers an item) simultaneously, using its object centric data model. | Ensures near instant transaction finality and high throughput (TPS), crucial for real time or fast paced game experiences. |
| [Programmable Transaction Blocks (PTBs)](https://docs.sui.io/concepts/transactions/prog-txn-blocks) | Allows developers to bundle up to 1,024 different smart contract calls into a single, atomic, and safe transaction. | Enables complex, multi step actions (e.g., "Use Item A to upgrade Item B, pay a fee, and earn an achievement") to execute reliably as one single operation. |

# II. Advanced Trust, Privacy & Data Storage
| Technology | Description | Video Game Utilization |
| --- | --- | --- |
| [Nautilus](https://docs.sui.io/concepts/cryptography/nautilus) | A system for confidential, verifiable offchain computation using Trusted Execution Environments (TEEs). | Secure Game Logic: Hosts hidden or high cost game logic (e.g., fog of war, secret auction bids, complex pathfinding) offchain while providing cryptographic proof of fair execution onchain. |
| [Seal](https://seal-docs.wal.app/) | A decentralized secrets management service offering robust encryption and onchain access control policies. | Gated Content/Quests: Encrypts mission data, item drops, or exclusive story content, which is only decrypted and revealed to a player upon meeting onchain conditions (e.g., owning a specific NFT or completing a task). |
| [Walrus](https://docs.wal.app/) | A decentralized storage and data availability protocol optimized for storing large, unstructured data files ("blobs"). | Decentralized Game Assets: Stores all large game assets (high resolution NFT imagery, 3D models, rich video trailers, patch files) in a decentralized, tamper proof manner, rather than on a centralized server. |

# III. Asset & Economy Features
| Technology | Description | Video Game Utilization |
| --- | --- | --- |
| [Dynamic NFTs (Mutable Assets)](https://docs.sui.io/guides/developer/nft-index)	| Assets where the metadata and attributes can change on chain over time without needing to mint a new token.	| A character's weapon can level up, a pet can evolve, or armor can gain stats based on in game actions, all reflected immediately on chain. |
| Composability / Nested Assets | The ability for one object to own another object (dynamic fields), creating complex hierarchies. | A player's Character NFT can hold (own) a Backpack NFT, which in turn holds Item NFTs (weapons, potions). |
| [Kiosk](https://docs.sui.io/standards/kiosk) | A decentralized, on chain system for commerce and asset trading. | Provides a ready made, secure foundation for building in game marketplaces, shops, and peer to peer trading features. |
| [Soulbound Assets](https://docs.sui.io/guides/developer/nft/nft-soulbound) | Assets that are created with a policy preventing their transfer or sale to another address. | Used for non tradeable achievements, story progression markers, quest items, or in game licenses. |
| [Sui Object Display Standard](https://docs.sui.io/standards/display) | A standard for defining how on chain assets should be visually represented in different apps and interfaces. | Ensures a consistent display (image, name, description) for in game items across the game, wallets, and external marketplaces. |
| [DeepBook](https://docs.sui.io/standards/deepbook) | A native, on chain central limit order book (CLOB). | Provides an efficient foundation for creating stable and liquid in game currency exchanges or token swaps. |

# IV. User Experience (UX) & Onboarding
| Technology | Description | Video Game Utilization |
| --- | --- | --- |
| [zkLogin (Account Abstraction)](https://docs.sui.io/concepts/cryptography/zklogin) | A Zero Knowledge Proof (ZKP) system that allows users to create and log into a Sui wallet using familiar Web2 credentials (e.g., Google or Facebook). | Removes the friction of seed phrases and complex wallet setup, enabling a seamless, Web2 like onboarding process for mainstream gamers. |
| [Sponsored Transactions](https://docs.sui.io/guides/developer/sui-101/sponsor-txn) | The ability for a developer or a designated entity to pay the gas fees for an end user's transactions (via Enoki or similar services). | Allows games to be truly "free to start" by covering the gas costs for initial players, removing the barrier of buying SUI tokens.
| [SuiNS / MVR](https://docs.suins.io/move-registry) | Sui Name Service (SuiNS) provides a human-readable, decentralized identity (sui.id) that replaces complex wallet addresses. Multi-Venue Resolution (MVR) ensures this single identity is consistently recognized and linked across various applications and smart contracts. | Enables gamers to use a simple, memorable username (e.g., GamerTag.sui) as their universal profile, friend identifier, and wallet address, creating a seamless and unified digital identity across all Sui-based games. |

# V. Development Tooling Partnerships
| Partnership/SDK | Description | Video Game Utilization |
| --- | --- | --- |
| [Venly Gaming SDK](https://www.venly.io/use-cases/gaming) | A comprehensive SDK for Web3 features that integrates with popular game engines like Unreal Engine and Unity. | Simplifies the implementation of wallet integration, NFT management, and marketplace features directly within existing game development workflows. |
| [Beamable](https://beamable.com/marketplace/sui) | A powerful LiveOps and game backend platform that offers a Sui integration in its Marketplace. | Provides a robust backend environment, enabling developers to build, manage, and scale games with integrated Sui functionality for asset and economy management. |

# VI. Core Mysten Tools and SDKs
| Tool / SDK | Description | Core Use for Development |
| --- | --- | --- |
| [Sui CLI](https://docs.sui.io/guides/developer/getting-started/sui-install) | The Command-Line Interface client that provides command-level access to the Sui network. It is used for low-level interactions like publishing Move smart contracts, getting object information, executing transactions, and managing addresses. | Essential for local development, contract deployment, and administrative tasks in a command-line environment. |
| [Walrus CLI / API](https://docs.wal.app/usage/interacting.html) | Walrus is an application management platform designed for platform engineering teams to manage application configuration, infrastructure orchestration, and environment setup. The CLI/API allows developers to interact with the Walrus server for these purposes. | Used by game services or platform teams to manage the underlying infrastructure and deployment environments, separating concerns from application developers. |
| [Seal TS SDK](https://seal-docs.wal.app/GettingStarted/) | A TypeScript SDK for interacting with the Walrus/Seal platform to manage application resources and configurations. | Allows game services to programmatically manage their deployment and configuration via TypeScript. |
| [Sui TS SDK](https://sdk.mystenlabs.com/typescript) | The core TypeScript SDK (@mysten/sui) providing all the low-level functionality needed to interact with the Sui ecosystem. It offers utility classes and functions for signing transactions and interacting with the Sui JSON RPC API. | The primary library for any client or service written in TypeScript/JavaScript that needs to read data or submit transactions to Sui. |
| [BCS TS SDK](https://sdk.mystenlabs.com/bcs) | TypeScript SDK for Binary Canonical Serialization (BCS). BCS is the serialization format used to represent the state of the Sui blockchain, guaranteeing a one-to-one correspondence between in-memory values and byte representations. | Used for low-level data serialization/deserialization of Move objects, transactions, and events, particularly when building custom tools or working closely with raw transaction data. |
| [Dapp Kit TS SDK](https://sdk.mystenlabs.com/dapp-kit) | A collection of React hooks, components, and utilities (@mysten/dapp-kit) that make building Sui dApps straightforward. Key features include query hooks for RPC calls, automatic wallet state management, and support for all Sui wallets. | Accelerates frontend development by providing pre-built components and hooks for wallet connection and querying blockchain data in React-based game clients or web portals. |
| [Kiosk TS SDK](https://sdk.mystenlabs.com/kiosk) | Tools for interacting with the Sui Kiosk standard, a decentralized system for commerce applications. Kiosks are shared objects that store assets and allow for listing them for sale while enabling creator-defined transfer policies (like royalty enforcement). | Crucial for in-game item trading and marketplace integration, ensuring assets are managed securely and creator policies are enforced on-chain. |
| [Payment Kit TS SDK](https://sdk.mystenlabs.com/payment-kit) | This SDK would simplify the creation and execution of transactions related to sending and receiving tokens (SUI or other assets) within the game.	| Simplifies the implementation of in-game purchases, transfers, and token management flows. |
| [Walrus TS SDK](https://sdk.mystenlabs.com/walrus) | The official toolkit for building web or backend applications in TypeScript that need to interact with Walrus's decentralized storage. It includes tools like the Upload Relay for optimized data uploads and native support for Quilt (for small file efficiency). | Used for off-chain data storage (e.g., game assets, user profiles) where reliability, user ownership via their wallet, and optimized uploads are required. |
| [zkSend TS SDK](https://sdk.mystenlabs.com/zksend) | The SDK provides the functionality to create your own zkSend Claim Links. This primitive allows for sending any publicly transferrable asset via a link, leveraging zero-knowledge proofs for simpler transfers. | Enables streamlined asset distribution and onboarding by letting developers or users send SUI or NFTs with a simple link, simplifying the recipient's claiming process. |
| [Enoki TS SDK](https://docs.enoki.mystenlabs.com/ts-sdk) | Integrates Enoki, an embedded wallet service, into dApps. It allows users to get a Sui address based on their Web 2.0 authentication (e.g., Google, Apple, Twitch), without needing to install a separate crypto wallet. | Drastically simplifies onboarding by letting users sign in with their Web2 credentials and perform on-chain transactions without managing cryptographic keys. |
| [Enoki Connect TS SDK](https://docs.enoki.mystenlabs.com/enoki-connect) | The connector package that enables a consistent way to handle signing transactions for Enoki wallets using the wallet-standard. It works in conjunction with dapp-kit to allow Enoki wallets to sign and execute transactions. | Connects Enoki's embedded wallet logic with the standard Sui wallet interfaces, ensuring compatibility with existing dApp tooling. |
| [Move Version Registry (MVR - pronounced “Mover”)](https://docs.suins.io/move-registry) | Move Version Registry (MVR), pronounced "mover," is a uniform naming service. It allows packages and types to be referenced by human-readable names in transactions and development, and also helps manage package versioning. | Improves developer experience by using names instead of complex addresses and helps manage smart contract upgrades/versions. |
| [Nautilus reference](https://docs.sui.io/concepts/cryptography/nautilus) | A framework for secure and verifiable off-chain computation on Sui, enabling developers to delegate sensitive or resource-intensive tasks to a Trusted Execution Environment (TEE) like AWS Nitro Enclaves. | Critical for complex and private game logic that is too resource-intensive or requires data privacy (e.g., complex AI agents, fraud prevention, secure private computations) while maintaining on-chain verification. |

# Additional Notes:
To learn more visit:
* [Sui Documentation](https://docs.sui.io/)
* [Gaming on Sui | Sui Documentation](https://docs.sui.io/concepts/gaming)
