---
title: About Sui
---

Sui is the first permissionless Layer 1 blockchain designed from the ground up to enable creators and developers to build experiences that cater to the next billion users in web3. Sui is horizontally scalable to support a wide range of application development with unrivaled speed at low cost. 
 
## Parallel agreement - a breakthrough in system design

Sui scales horizontally with no upper bound to meet application demand while maintaining extremely low operating costs per transaction. Its system design breakthrough eliminates a critical bottleneck in existing blockchains: the need to achieve global consensus on a total-ordered list of transactions. This computation is wasteful given most transactions are not contending for the same resource against other transactions.

Sui takes a significant leap in scalability by enabling parallel agreement on causally independent transactions. Sui authorities commit such transactions using Byzantine consistent broadcast, eliminating global consensus's overhead without sacrificing safety and liveness guarantees.

This breakthrough is possible only with Sui's novel data model. Thanks to its object-centric view and Move’s strong ownership types, dependencies are explicitly encoded. As a result, Sui both agrees on and executes transactions on most objects in parallel, while a minority of transactions that affect shared state are ordered via Byzantine fault tolerant consensus and executed in parallel.

### Highlights

* Unmatched scalability, instant settlement
* A safe smart contract language accessible to mainstream developers
* Ability to define rich and composable on-chain assets
* Better user experience for web3 apps

Sui is the only blockchain today that can scale with the growth of web3 while achieving industry-leading performance, cost, programmability, and usability. As we push towards mainnet launch, we will demonstrate capacity beyond the transaction processing capabilities of established systems – traditional and blockchain alike. We see Sui as the first internet-scale programmable blockchain platform, a foundational layer for web3.  

## Unparalleled scalability, immediate settlement

Today, users of existing blockchains pay a considerable tax as network usage increases due to limited throughput. In addition, high latency limits the responsiveness of applications. These factors contribute to the bad user experiences that are all too common in web3:

* Games are slow and prohibitively expensive to play
* Investors lose funds when they can’t liquidate undercollateralized loans in Decentralized Finance (DeFi)
* High-volume, low-value, per-transaction mass-market services like micropayments and coupons are priced out of the network
* Artificially high floor prices on assets due to high gas prices

Sui scales horizontally to meet the demands of applications. Network capacity grows in proportion to the increase in Sui authorities' processing power by adding workers, resulting in low gas fees even during high network traffic. This scalability characteristic is in sharp contrast to other blockchains with rigid bottlenecks.

By design, Sui authorities (nodes) can effectively scale the network throughput infinitely to meet the demand of builders and creators. We believe Sui can do for web3 what broadband internet did for web2. 

Note: As of Mar 12, 2022, an unoptimized single-worker Sui authority running on an 8-core M1 Macbook Pro can process 17,500 token transfer transactions per second (TPS). Performance scales linearly with the number of cores–the same machine processes 3,000 TPS in a single core configuration and increases by about 2,000 TPS with each additional core. . 

We will publish a full performance report for optimized Sui networks of various sizes when our testnet is released.

## A safe smart contract language accessible to mainstream developers

Move smart contracts power Sui applications. Move is a programming language initially developed at Facebook for writing safe smart contracts. It is a platform-agnostic language that enables shared libraries, tooling, and developer communities across blockchains. 

Move's design prevents issues such as [reentrancy](https://en.wikipedia.org/wiki/Reentrancy_(computing)) vulnerabilities, [poison tokens](https://www.theblockcrypto.com/post/112339/creative-attacker-steals-76000-in-rune-by-giving-out-free-tokens)), and [spoofed token approvals](https://www.theverge.com/2022/2/20/22943228/opensea-phishing-hack-smart-contract-bug-stolen-nft)) that attackers have leveraged to steal millions on other platforms. Its emphasis on safety and expressivity makes it easier for developers to transition from web2 to web3 without understanding the intricacies of the underlying infrastructure.

We are confident that Move is well-positioned to become the de-facto execution environment not only for Sui but for every next-generation smart contract platform.

## Ability to define rich and composable on-chain assets

Sui’s scalability is not limited to transaction processing. Storage is also low-cost and horizontally scalable. This enables developers to define complex assets with rich attributes that live directly on-chain instead of introducing layers of indirection into off-chain storage to save on gas fees. Moving attributes on-chain unlocks the ability to implement application logic that uses these attributes in smart contracts, increasing composability and transparency for applications.

Rich on-chain assets will enable new applications and economies based on utility without relying solely on artificial scarcity. Developers can implement dynamic NFTs that can be upgraded, bundled, and grouped in an application-specific manner, such as changes in avatars and customizable items based on gameplay. This capability delivers stronger in-game economies as NFT behavior gets fully reflected on-chain, making NFTs more valuable and delivering more engaging feedback loops.

## Better user experience for web3 apps

We want to make Sui the most accessible smart contract platform, empowering developers to create great user experiences in web3. To usher in the next billion users, we will empower developers with various tools to take advantage of the power of the Sui blockchain. The Sui Development Kit (SDK) will enable developers to build without boundaries.

## Build cool stuff

Here are some cool things you can do now and some applications that will become possible over the next few weeks and months. Sui enables developers to define and build:

* On-chain DeFi and Traditional Finance (TradFi) primitives:  enabling real-time, low latency on-chain trading
* Reward and loyalty programs: deploying mass airdrops that reach millions of people through low-cost transactions
* Complex games and business logic: implementing on-chain logic transparently, extending the functionality of assets, and delivering value beyond pure scarcity
* Asset tokenization services: making ownership of everything from property deeds to collectibles to medical and educational records perform seamlessly at scale
* Decentralized social media networks: empowering creator-owned media, posts, likes, and networks with privacy and interoperability in mind
