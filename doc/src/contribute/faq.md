---
title: Sui Frequently Asked Questions
---

This page contains answers to frequently asked questions (FAQs) about Sui and Mysten Labs. 
Ask more in [Discord](https://discord.gg/mysten), see where we are headed in our
[roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md#roadmap), and
find full details in our
[white paper](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf).


## Why Sui?


### Where can I find out about Sui?

Use these online resources:

* Sui Website: [https://sui.io/](https://sui.io/)
* Sui Developer Portal: [https://docs.sui.io/](https://docs.sui.io/)
* Move Programming Language: [https://docs.sui.io/build/move](../build/move.md)
* Sui Smart Contract White Paper: [https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) 
* SDK reference: [https://app.swaggerhub.com/apis/MystenLabs/sui-api/](https://app.swaggerhub.com/apis/MystenLabs/sui-api/)  


### What does Sui offer over other blockchains?

Sui offers ease of development, a developer interface, fast transaction speeds, a sane object model, and better security. Sui calls the consensus protocol only for transactions affecting objects owned by multiple accounts. This means simple sends can happen almost immediately.

See these resources on the [Sui Developer Portal](https://docs.sui.io/) for the complete story on why we built Sui:



* [Why Move?](../learn/why-move)
* [How Sui Move differs from Core Move](../learn/sui-move-diffs)
* [How Sui Works](../learn/how-sui-works)
* [Sui Compared to Other Blockchains](../learn/sui-compared)


### Is Sui based on Diem?

There is no technical relationship between Diem and Sui except that both use Move.

All five co-founders (as well as several Mysten employees) worked on the Diem system and are very familiar with both its good qualities and its limitations. Diem was designed to handle light payments traffic between a small number (10s-100s) of custodial wallets. There were eventual visions of evolving it into a more scalable system that is capable of handling more general-purpose smart contracts; however, the original architecture was not designed to support this and has not evolved significantly.

When we started Mysten, we had the option to build on top of Diem but chose not to because of these limitations. We believe blockchain tech has evolved a lot since Diem came out in 2019, and we have many ideas about how to design a system that is more scalable and programmer-friendly from the ground up. That is why we built Sui.


### What is the relationship between Sui/Mysten and Aptos?

There is no relationship between Sui/Mysten and Aptos. The similarity is that they both use Move; but Sui has a different object model. The research behind the [block STM paper](https://arxiv.org/abs/2203.06871) was all done at Facebook. Subsequently, some of the authors joined Mysten and some joined Aptos. The paper carries the current affiliations of the authors.


## Roadmap


### Can I buy the Sui token? Is Mysten labs' token name SUI?

There is currently no timeline for a public Sui token sale. We will have a public token by the time mainnet launches, but it is not available right now and there is no timeline as of yet. Anyone who claims otherwise (offering tokens, whitelists, pre-sale, etc.) is running a scam." 


### When is the Sui devnet/testnet/mainnet launching?

A testnet is coming in a few months. See our roadmap: \
[https://github.com/MystenLabs/sui/blob/main/ROADMAP.md#roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md#roadmap) 


### Is there some kind of waitlist for the testnet?

No. More information is forthcoming.


## Getting involved {#getting-involved}


### How can I join the Sui network? How do I participate in the Sui project?

Here are the current participation methods:

1. Download the [Sui SDK](https://app.swaggerhub.com/apis/MystenLabs/sui-api/) and start building.
1. Join the [Move](https://discord.gg/8prNjUqyFj) and [Sui](https://discord.gg/CVcnUzKYCB) developer channels in Discord.
1. Use the SDK for [testing packages](https://docs.sui.io/build/move#testing-a-package).


### Are you looking for partners?

We are seeking partners that can contribute to the ecosystem primarily in development by building apps with the SDK now so they can be ready to launch when the network goes live.


### With whom can I talk about a partnership or strategic investment? How can I discuss Sui in person?
Start in [Discord](https://discord.gg/mysten). 


### Where can I see the investors of the project?

See the [mystenlabs.com](https://mystenlabs.com/) website for company details.


### Do you need moderators in Discord? Can I be the mod for my country?

Wait until the community managers are on board. The managers will plan for growing and engaging the community. Stay tuned in Discord.


### I'm looking for someone from the Mysten Crew to speak at a student club web3 event - is there someone I can DM/email?

Ask in Discord.


## Development


### Are there things we can already test if we are not a developer?

We have a number of examples and demos available for viewing at: [https://docs.sui.io/explore](../explore) 


### Once a Move package is published to the Sui network, is there any way for devs to update the code?

Not currently. Packages are immutable objects, and this property is relied upon in several places.


### Is there any information on node architecture and running validators on Sui?

Section four in the[ Sui Smart Contract Platform](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) white paper is the best reference for node architecture.


### Can I run a Sui validator node?

We do not yet have a public devnet or testnet that will allow others to operate Sui nodes. You can run a Sui node or Sui network locally. Check out our [Wallet](https://docs.sui.io/build/wallet) documentation and then speak to it either using the [Wallet CLI](../build/wallet#command-line-mode) or [REST API](../build/rest-api).

You can run the [SDK](https://app.swaggerhub.com/apis/MystenLabs/sui-api/) for a local, non-networked node for development. We do not yet have a public devnet or testnet that will allow others to operate Sui nodes, but both are on our [roadmap](https://github.com/MystenLabs/sui/blob/main/ROADMAP.md#roadmap).


### What does a local node do and what hardware requirements will I be needing to run it? How is it different from the coming testnet node?

A local node allows you to build software using Move for Sui. On a single core m1, it should run 30k TPS. Testnet will be running on a network of validators. So, with enough nodes or validators running on mainnet..Sui will do 30k tps blazing speeds. An 8-core machine runs 120k TPS unoptimized for simple transactions.


## Technology


### Is Sui compatible with Ethereum Virtual Machine (EVM)?

No. Sui heavily leverages the Move's asset-centric data model for its novel parallel execution and commitment scheme. This is simply not possible with the EVM data model. Because assets are represented as entries in dynamically indexable maps, it is not possible to statically determine which assets a transaction will touch.

To be blunt: even if we preferred the EVM/Solidity to Move, we could not use them in Sui without sacrificing the performance breakthroughs that make Sui unique. And of course, we think there are many reasons why Move is a safer and more developer-friendly language than the EVM.

See [Why move?](../learn/why-move) for more details on this. In addition, see the [Move Problem Statement](https://github.com/MystenLabs/awesome-move/blob/main/docs/problem_statement.md) for why we think that - despite being the most popular smart contract language of today - EVM is holding back the crypto space.

Finally, the EVM developer community is very small--approximately 4,000 programmers according to [this study](https://medium.com/electric-capital/electric-capital-developer-report-2021-f37874efea6d). Compare this to (e.g.) the [>20M registered iOS developers](https://techcrunch.com/2018/06/04/app-store-hits-20m-registered-developers-at-100b-in-revenues-500m-visitors-per-week/#:~:text=Today%20at%20WWDC%2C%20Apple's%20CEO,500%20million%20visitors%20per%20week.). Thus, the practical path to scaling the smart contract dev community is to bring folks in from the broader population, not to pull them from the tiny pool of existing Solidity developers. We think Move is much safer and much more approachable for mainstream programmers than the EVM.


### Can you tell me all about Sui’s shared object consensus?

Q: I have been looking into your project since the announcement and have a question about this comment from the white paper:

“When full agreement is required we use a high-throughput DAG-based consensus, e.g. [9] to manage locks, while execution on different shared objects is parallelized.”

The referenced paper is Narwhal and Tusk. I understand that Narwhal enables the parallel ordering of transactions into batches which are collected into concurrently proposed blocks, and that Tusk defines an algorithm for executing the DAG that these blocks form. Is your consensus for shared-object transactions a direct implementation of Narwhal and Tusk, or have you made some modifications to enable the parallel execution of transactions that touch independent sets of shared objects? How do you ensure that the DAG encodes the dependencies between the different sets of objects referenced by each transaction (given that these dependencies may change according to the order that the transactions are executed in)? If the DAG does not encode these dependencies, then how do you identify them? 

For context, I am assuming that Sui-Move allows Smart Contracts to define and modify internal variables (i.e. variables not referenced in the transaction data) and supports conditional flows

A: - Narwhal/Tusk (N/T) builds a DAG of blocks, indeed concurrently proposed, and creates an order between those blocks as a byproduct of the building of the DAG. But that order is overlaid on top of the causal order of Sui transactions (the "payload" of Narwhal/Tusk here), and does not substitute for it.

- Narwhal/Tusk operates in OX, rather than XO mode (O = order, X = execute): the execution occurs after the Narwhal/Tusk ordering.

- The output of N/T is therefore a sequence of transactions, with interdependencies stored in the transaction data itself.

What we sequence using consensus is certificates of transactions. These represent transactions that have already been presented to 2/3 of authorities, that checked that all their owned objects are available to be operated on, and signed the transaction. Upon a certificate being sequenced, what we do is set the "lock" of the shared objects at the next available version to map to the execution of that certificate. So for example if we have a shared object X at version 2, and we sequence certificate T, we store T -> [(X, 2)]. That is all we do synchronously when we reach consensus, and as a result we are able to ingest a lot of sequenced transactions.

Now, once this is done we can process all certificates that had their locks set, on one or multiple cores (currently). Obviously, transactions for earlier versions of objects need to be processed first (causally), and that reduces the degree of concurrency. Both the read and write set is determined by the transaction itself, and not dynamically based on the contents of the object at a specific version (not currently). 

It is an interesting question whether allowing dynamic read / write set inference might bring benefits -- we will have to see the nature of contracts and how they are implemented to understand this better.


### Is there a difference in principle between Solana Sealevel and Sui execution?
Q:

For transactions involving shared objects, the broad strokes look similar

obviously every Solana transaction would go through the consensus mechanism in Solana

A: As far as parallelism goes:

Solana does optimistic concurrency control, which is to launch a bunch of TX executions in parallel, and retry in some sequence those for which that didn't work. One difference is they're discovering the causal dependencies between TXes: by the time a TX with dependencies is run successfully, that may be its second (or more) execution. 

For Sui, the causality is declared up front and the TX is run after its dependents. (You might call this "pessimistic" by contrast.)

Naturally, as you point out, there are many, many other differences the moment you zoom out of the precise question of concurrent execution (degree of consensus involvement, flow of the TX to an authority, leaderless architecture, etc.).

Solana declares a weaker form of concurrency, that is state registers the TX will access in either mode, and that's a useful heuristic for sure. We declare object versions the execution engine must have before executing.


### Is Sui an L2, or are there plans to support L2s?

Sui tackles scaling at the base layer rather than via L2s. We think this approach leads to a more user and developer-friendly system than adding additional complexity on top of an already-complex base layer that doesn't scale.


### Does Mysten maintain a fork of Move?

No. Move is designed to be a cross-platform language that can be used anywhere you need safe smart contracts. There are some more details on how this works + the chains Move runs in the [Awesome Move](https://github.com/MystenLabs/awesome-move) documentation.
