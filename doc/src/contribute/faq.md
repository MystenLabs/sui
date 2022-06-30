---
title: Sui Frequently Asked Questions
---

This page contains answers to frequently asked questions (FAQs) about Sui and Mysten Labs. 
Ask more in [Discord](https://discord.gg/sui), see where we are headed in our
[roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md#roadmap), and
find full details in our
[white paper](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf).


## Why Sui?


### Where can I find out about Sui?

Use these online resources:

* Sui Website: [https://sui.io/](https://sui.io/)
* Sui Developer Portal: [https://docs.sui.io/](https://docs.sui.io/)
* Sui Smart Contract White Paper: [https://sui.io/whitepaper](https://sui.io/whitepaper) 
* SDK reference: [Sui JSON-RPC](https://playground.open-rpc.org/?uiSchema%5BappBar%5D%5Bui:splitView%5D=false&schemaUrl=https://raw.githubusercontent.com/MystenLabs/sui/main/sui/open_rpc/spec/openrpc.json&uiSchema%5BappBar%5D%5Bui:input%5D=false)  


### What does Sui offer over other blockchains?

Sui offers ease of development, a developer interface, fast transaction speeds, a sane object model, and better security. Sui calls the consensus protocol only for transactions affecting objects owned by multiple accounts. This means simple transactions complete almost immediately.

See these resources on the [Sui Developer Portal](https://docs.sui.io/) for the complete story on why we built Sui:



* [Why Move?](../learn/why-move)
* [How Sui Move differs from Core Move](../learn/sui-move-diffs.md)
* [How Sui Works](../learn/how-sui-works.md)
* [Sui Compared to Other Blockchains](../learn/sui-compared.md)


### Is Sui based on Diem?

There is no technical relationship between Diem and Sui except that both use Move.

All five co-founders (as well as several Mysten employees) worked on the Diem system and are very familiar with both its good qualities and its limitations. Diem was designed to handle light payments traffic between a small number (10s-100s) of custodial wallets. There were eventual visions of evolving it into a more scalable system that is capable of handling more general-purpose smart contracts; however, the original architecture was not designed to support this and has not evolved significantly.

When we started Mysten, we had the option to build on top of Diem but chose not to because of these limitations. We believe blockchain tech has evolved a lot since Diem came out in 2019, and we have many ideas about how to design a system that is more scalable and programmer-friendly from the ground up. That is why we built Sui.


### What is the relationship between Sui/Mysten and Aptos?

There is no relationship between Sui/Mysten and Aptos. The similarity is that they both use Move; but Sui has a different object model. The research behind the [block STM paper](https://arxiv.org/abs/2203.06871) was all done at Facebook. Subsequently, some of the authors joined Mysten and some joined Aptos. The paper carries the current affiliations of the authors.


## Roadmap


### Can I buy the Sui token?

We will have a public token for the Sui mainnet. But it is not available right now and there is no timeline as of yet. Anyone who claims otherwise (offering tokens, whitelists, pre-sale, etc.) is running a scam.

### Will the token name be SUI?

Yes, the token name will be SUI.


### When is the Sui devnet/testnet/mainnet launching?

We have launched our [Sui DevNet](../explore/devnet.md) in May 2022. A testnet is coming in a few months. 

See our roadmap: \
[https://github.com/MystenLabs/sui/blob/main/ROADMAP.md#roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md#roadmap) 


### Is there some kind of waitlist for the testnet?

No. More information is forthcoming.


## Getting involved


### How can I join the Sui network? How do I participate in the Sui project?

Join our [Discord](https://discord.gg/sui) and follow our [Twitter](https://twitter.com/Mysten_Labs) for the latest updates and announcements.

For developers:

1. [Install Sui](https://docs.sui.io/build/install).
2. Spin up a [Sui Fullnode](../build/fullnode.md).
3. Experiment with the [Sui CLI client](../build/cli-client.md).
4. Join the [Move](https://discord.gg/8prNjUqyFj) and [Sui](https://discord.gg/CVcnUzKYCB) developer channels in Discord.
5. [Start building](https://docs.sui.io/build)!

### Are there things we can already try if we are not a developer?

We have a number of examples and demos available for viewing at:
https://docs.sui.io/explore

### Are you looking for partners?

We are seeking partners that can contribute to the ecosystem primarily in development by building apps with the SDK now so they can be ready to launch when the network goes live. If interested, please apply using the [Sui Partnerships Form](https://bit.ly/suiform).


### With whom can I talk about a partnership or strategic investment? How can I discuss Sui in person?
Start in [Discord](https://discord.gg/sui).


### Where can I see the investors of the project?

See the [mystenlabs.com](https://mystenlabs.com/) website for company details.


### Do you need moderators in Discord? Can I be the mod for my country?

Sui Community Mod Program is officially accepting applications. [Apply here](https://bit.ly/suimods)


### I'm looking for someone from the Mysten Crew to speak at an event - is there someone I can DM/email?

Ask in Discord.


## Development

### Once a Move package is published to the Sui network, is there any way for devs to update the code?

Not currently. Packages are immutable objects, and this property is relied upon in several places.


### Is there any information on node architecture and running validators on Sui?

Section four in the [Sui Smart Contract Platform](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) white paper is the best reference for node architecture.

See the instructions to [run a Sui Fullnode](../build/fullnode.md).


### Can I run a Sui validator node?

We have a public [Sui DevNet](../explore/devnet.md) with nodes operated by Mysten Labs. And you can now run a networked [Sui Fullnode](../build/fullnode.md).

You may also run a local, non-networked Sui node for development using the [Sui CLI client](../build/cli-client.md). You can interact with the nodes with either the [Sui Wallet CLI](../build/cli-client.md#command-line-mode) or over the [Sui RPC API](../build/json-rpc.md).

Soon we will have a TestNet that allow others to operate Sui nodes, but both are on our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md#roadmap). Right now, 

### What does a local node do and what are the hardware requirements to run it? How is it different from a testnet node?

A local node allows you to immediately start building software using Move for Sui, and can be run on commodity hardware. See the [Sui Fullnode](../build/fullnode.md) documentation for system requirements.

A testnet node will be part of a live network of validators.


## Technology


### Is Sui compatible with Ethereum Virtual Machine (EVM)?

No. Sui heavily leverages the Move's asset-centric data model for its novel parallel execution and commitment scheme. This is simply not possible with the EVM data model. Because assets are represented as entries in dynamically indexable maps, it is not possible to statically determine which assets a transaction will touch.

To be blunt: even if we preferred the EVM/Solidity to Move, we could not use them in Sui without sacrificing the performance breakthroughs that make Sui unique. And of course, we think there are many reasons why Move is a safer and more developer-friendly language than the EVM.

See [Why move?](../learn/why-move.md) for more details on this. In addition, see the [Move Problem Statement](https://github.com/MystenLabs/awesome-move/blob/main/docs/problem_statement.md) for why we think that - despite being the most popular smart contract language of today - EVM is holding back the crypto space.

Finally, the EVM developer community is very small--approximately 4,000 programmers according to [this study](https://medium.com/electric-capital/electric-capital-developer-report-2021-f37874efea6d). Compare this to (e.g.) the [>20M registered iOS developers](https://techcrunch.com/2018/06/04/app-store-hits-20m-registered-developers-at-100b-in-revenues-500m-visitors-per-week/#:~:text=Today%20at%20WWDC%2C%20Apple's%20CEO,500%20million%20visitors%20per%20week.). Thus, the practical path to scaling the smart contract dev community is to bring folks in from the broader population, not to pull them from the tiny pool of existing Solidity developers. We think Move is much safer and much more approachable for mainstream programmers than the EVM.

### Is Sui an L2, or are there plans to support L2s?

Sui tackles scaling at the base layer rather than via L2s. We think this approach leads to a more user and developer-friendly system than adding additional complexity on top of an already-complex base layer that doesn't scale.


### Does Mysten maintain a fork of Move?

No. Move is designed to be a cross-platform language that can be used anywhere you need safe smart contracts. There are some more details on how this works + the chains Move runs in the [Awesome Move](https://github.com/MystenLabs/awesome-move) documentation.
