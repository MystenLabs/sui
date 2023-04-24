---
title: Sui Frequently Asked Questions
---

This page contains answers to frequently asked questions (FAQs) about Sui and Mysten Labs. 
Ask more in the [Sui Discord](https://discord.gg/sui) server.

## What does Sui offer over other blockchains?

Sui offers ease of development, a developer interface, fast transaction speeds, a sane object model, and better security. Sui calls the [consensus protocol](../learn/architecture/consensus.md) only for transactions affecting objects owned by multiple addresses. This means simple transactions complete almost immediately.

Additional resources:

 * [Why Move?](../learn/why-move)
 * [How Sui Move differs from Core Move](../learn/sui-move-diffs.md)
 * [How Sui Works](../learn/how-sui-works.md)
 * [Sui Compared to Other Blockchains](../learn/sui-compared.md)
 * [Narwhal and Bullshark, Sui's Consensus Engine](../learn/architecture/consensus.md)

## Can I buy Sui tokens?

SUI will be listed on public exchanges after the Mainnet launch.

## How can I join the Sui network? How do I participate in the Sui project?

Join our [Discord](https://discord.gg/sui) and follow our [Twitter](https://twitter.com/SuiNetwork) for the latest updates and announcements.

You can also join the [Move](https://discord.gg/8prNjUqyFj) and [Sui](https://discord.gg/CVcnUzKYCB) developer channels in Discord.

## Are you looking for partners?

If interested in partnering with Sui, please apply using the [Sui Partnerships Form](https://bit.ly/suiform).

## After I publish a Move package, how do I update it?

Packages are immutable objects, and this property is relied upon in several places. To update the package you need to publish an updated package.

## Is there any information on node architecture and running validators on Sui?

See the [Sui Smart Contract Platform](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) for node architecture information.

See the instructions to [run a Sui Full node](../build/fullnode.md).

## Is Sui compatible with Ethereum Virtual Machine (EVM)?

No. Sui heavily leverages the Move's asset-centric data model for its novel parallel execution and commitment scheme. This is simply not possible with the EVM data model. Because assets are represented as entries in dynamically indexable maps, it is not possible to statically determine which assets a transaction will touch.

Further, Sui could not implement EVMs without sacrificing the performance breakthroughs that make Sui unique. There are also many reasons why Move is a safer and more developer-friendly language than the EVM.

See [Why move?](../learn/why-move.md) for more details on this. In addition, see the [Move Problem Statement](https://github.com/MystenLabs/awesome-move/blob/main/docs/problem_statement.md) for why - despite being the most popular smart contract language of today - EVM is holding back the crypto space.

Finally, the EVM developer community is very small--approximately 4,000 programmers according to [this study](https://medium.com/electric-capital/electric-capital-developer-report-2021-f37874efea6d). Compare this to (e.g.) the [>20M registered iOS developers](https://techcrunch.com/2018/06/04/app-store-hits-20m-registered-developers-at-100b-in-revenues-500m-visitors-per-week). Thus, the practical path to scaling the smart contract dev community is to bring folks in from the broader population, not to pull them from the tiny pool of existing Solidity developers. Sui believes that Move is much safer and much more approachable for mainstream programmers than the EVM.

## Is Sui an L2, or are there plans to support L2s?

Sui tackles scaling at the base layer rather than via L2s. This approach leads to a more user and developer-friendly system than adding additional complexity on top of an already-complex base layer that doesn't scale.
