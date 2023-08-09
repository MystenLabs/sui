---
title: About Sui
---

Sui is a [smart contract](sui-glossary.md#smart-contract) platform maintained by a permissionless set of [validators](sui-glossary.md#validator) that play a role similar to validators or miners in other blockchain systems.

Sui offers scalability and unprecedented low-latency for simple use cases. Sui makes most transactions processable in parallel. This better utilizes processing resources and offers the option to increase throughput by adding more resources. Sui forgoes consensus to instead use simpler and lower-latency primitives for simple use cases, such as payment transactions and assets transfer. This is unprecedented in the blockchain world and enables a number of new latency-sensitive distributed applications ranging from gaming to retail payment at physical points of sale.

Sui is written in [Rust](https://www.rust-lang.org) and supports smart contracts written in Sui Move&mdash;a powerful asset-centric adaptation of [Move](https://golden.com/wiki/Move_(programming_language)-MNA4DZ6) for the Sui blockchain&mdash;to define assets that may have an owner. Sui Move programs define operations on these assets, including: custom rules for their creation, the transfer of these assets to new owners, and operations that mutate assets. To learn about the differences between core Move and Sui move, see [How Sui Move differs from Core Move](../learn/sui-move-diffs.md).

## Sui tokens and validators

Sui has a native token called SUI, with a fixed supply. The SUI token is used to pay for gas, and users can stake their SUI tokens with validators in a [Delegated Proof-of-Stake](https://learn.bybit.com/blockchain/delegated-proof-of-stake-dpos/) model within an epoch. The voting power of validators within this epoch is a function of the amount of SUI in their staking pool, including both validator and user SUI tokens. In any epoch, the set of validators is [Byzantine fault tolerant](https://pmg.csail.mit.edu/papers/osdi99.pdf). At the end of the epoch, fees collected through all transactions processed are distributed to validators according to their contribution to the operation of the network. Validators can in turn share some of the fees as rewards to users that stake their SUI with them.

Sui is backed by a number of state-of-the-art [peer-reviewed works](../contribute/research-papers.md) and years of open source development.

## Transactions

A transaction in Sui is a change to the blockchain. This may be a *simple transaction* affecting only single-owner, single-address objects, such as minting an NFT or transferring an NFT or a different token. These *simple transactions* may bypass the consensus protocol in Sui.

More *complex transactions* affecting objects that are shared or owned by multiple addresses, such as asset management and other DeFi use cases, go through the [Narwhal and Bullshark](https://github.com/MystenLabs/sui/tree/main/narwhal) DAG-based mempool and efficient Byzantine Fault Tolerant (BFT) consensus.

## Parallel agreement - a breakthrough in system design

Sui scales horizontally with no upper bound to meet application demand while maintaining extremely low operating costs per transaction. Its system design breakthrough eliminates a critical bottleneck in existing blockchains: the need to achieve global consensus on a total-ordered list of transactions. This computation is wasteful given many transactions are not contending for the same resource against other transactions.

Sui takes a significant leap in scalability by enabling parallel agreement on causally independent transactions. Sui validators commit such transactions using Byzantine Consistent Broadcast, eliminating the overhead of global consensus without sacrificing safety and liveness guarantees.

This breakthrough is possible only with Sui's novel data model. Thanks to its object-centric view, and Move’s strong ownership types, dependencies are explicitly encoded. As a result, Sui both agrees on, and executes transactions on many objects in parallel. Meanwhile, transactions that affect shared state are ordered via Byzantine Fault Tolerant consensus and executed in parallel.

## Sui highlights

- Unmatched scalability, instant settlement
- A safe smart contract language accessible to mainstream developers
- Ability to define rich and composable on-chain assets
- Better user experience for web3 apps
- [Narwhal and Bullshark](../learn/architecture/consensus.md) DAG-based mempool and efficient Byzantine Fault Tolerant (BFT) consensus

Sui is the only blockchain today that can scale with the growth of web3 while achieving industry-leading performance, cost, programmability, and usability. Sui is the first internet-scale programmable blockchain platform, a foundational layer for web3.

## Unparalleled scalability, immediate settlement

Today, users of existing blockchains pay a considerable tax as network usage increases due to limited throughput. In addition, high latency limits the responsiveness of applications. These factors contribute to the poor user experiences that are all too common in web3:

 * Games are slow and prohibitively expensive to play
 * Investors lose funds when they can’t liquidate under-collateralized loans in Decentralized Finance (DeFi)
 * High-volume, low-value, per-transaction mass-market services like micro-payments and coupons are priced out of the network
 * Artificially high floor prices on assets due to high gas prices

Sui scales horizontally to meet the demands of applications. Network capacity grows in proportion to the increase in Sui validators' processing power by adding workers, resulting in low gas fees even during high network traffic. This scalability characteristic is in sharp contrast to other blockchains with rigid bottlenecks.

By design, Sui validators (nodes) can effectively scale the network throughput infinitely to meet the demand of builders and creators. Sui can do for web3 what broadband internet did for web2.

**Note:** As of Mar. 19, 2022, an non-optimized single-worker Sui validator running on an 8-core M1 Macbook Pro can execute and commit 120,000 token transfer transactions per second (TPS). Throughput scales linearly with the number of cores–the same machine processes 25,000 TPS in a single core configuration.

This experiment uses a configuration where each client submits a batch of 100 transactions (such as transfers to 100 distinct recipients) with a single signature. This configuration captures the anticipated usage pattern of a highly scalable blockchain--for example, a custodial wallet or game server operating at scale will likely need to submit hundreds or thousands of on-chain transactions per second. With a batch size of 1, a validator running on the same machine can process 20,000 TPS with 8 cores, and exhibits the same linear growth in throughput as more cores are added.

## A safe smart contract language accessible to mainstream developers

Sui Move smart contracts power Sui applications. Sui Move is a dialect of the Move programming language initially developed at Facebook for writing safe smart contracts. It is a platform-agnostic language that enables shared libraries, tooling, and developer communities across blockchains.

Sui Move's design prevents issues such as [reentrancy vulnerabilities](https://en.wikipedia.org/wiki/Reentrancy_(computing)), [poison tokens](https://www.theblock.co/post/112339/creative-attacker-steals-76000-in-rune-by-giving-out-free-tokens), and [spoofed token approvals](https://www.theverge.com/2022/2/20/22943228/opensea-phishing-hack-smart-contract-bug-stolen-nft) that attackers have leveraged to steal millions on other platforms. Its emphasis on safety and expressivity makes it easier for developers to transition from web2 to web3 without understanding the intricacies of the underlying infrastructure.

Sui Move is well-positioned to become the de-facto execution environment not only for Sui but for every next-generation smart contract platform.

## Ability to define rich and composable on-chain assets

Sui’s scalability is not limited to transaction processing. Storage is also low-cost and horizontally scalable. This enables developers to define complex assets with rich attributes that live directly on-chain instead of introducing layers of indirection into off-chain storage to save on gas fees. Moving attributes on-chain unlocks the ability to implement application logic that uses these attributes in smart contracts, increasing composability and transparency for applications.

Rich on-chain assets enable new applications and economies based on utility without relying solely on artificial scarcity. Developers can implement dynamic NFTs that can be upgraded, bundled, and grouped in an application-specific manner, such as changes in avatars and customizable items based on gameplay. This capability delivers stronger in-game economies as NFT behavior gets fully reflected on-chain, making NFTs more valuable and delivering more engaging feedback loops.

## Better user experience for web3 apps

Sui aims to make Sui the most accessible smart contract platform, empowering developers to create great user experiences in web3. To usher in the next billion users, Sui empowers developers with various tools to take advantage of the power of the Sui blockchain. The Sui Development Kit (SDK) will enable developers to build without boundaries.

## Build cool stuff

Here are some cool things you can do now and some applications that will become possible over the next few weeks and months. Sui enables developers to define and build:

 * On-chain DeFi and Traditional Finance (TradFi) primitives:  enabling real-time, low latency on-chain trading
 * Reward and loyalty programs: deploying mass airdrops that reach millions of people through low-cost transactions
 * Complex games and business logic: implementing on-chain logic transparently, extending the functionality of assets, and delivering value beyond pure scarcity
 * Asset tokenization services: making ownership of everything from property deeds to collectibles to medical and educational records perform seamlessly at scale
 * Decentralized social media networks: empowering creator-owned media, posts, likes, and networks with privacy and interoperability in mind
