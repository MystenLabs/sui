---
title: Experiment with Sui DevNet
---

Welcome to the beginnings of the Sui DevNet. It exists now to gain operational experience with the Sui software in a public setting. The Sui DevNet currently consists of:

* A four-validator network with all nodes operated by Mysten Labs. Clients send transactions and read requests via this endpoint: http://gateway.devnet.sui.io/
* A public network [Sui Explorer](https://github.com/MystenLabs/sui/tree/explorer-rest/explorer) for browsing the TestNet transaction history: http://explorer.devnet.sui.io
* A [Discord channel](https://discordapp.com/channels/916379725201563759/971488439931392130) for requesting test coins that can be used to pay for gas on the test network. These coins have no financial value and will disappear each time we reset the network.

Many improvements to the Sui DevNet are underway, such as the ability to run FullNodes and use a browser-based wallet. See the Sui DevNet blog post announcement for full details on upcoming features.

TODO: Create and link to Medium blog post.

## Purpose

This procedure leverages the following components of the Sui blockchain:

- The Wallet CLI
    - creating and managing your private keys
    - submitting transactions for creating example NFTs on SUI
    - calling/publishing Move modules
- The Gas Faucet service
    - Transfer SUI tokens to you so that you can pay for the transactions
- SuiExplorer
    - View transactions and objects

TODO: Edit, format, and link from the text above to relevant pages once we agree to include it. No faucet docs will exist for DevNet per Chris.

## Prerequisites

### Set up environment

To use the Sui DevNet, you will need:

* the [`git` command line interface](https://git-scm.com/download/)
* coins requested through Discord (need channel link)
* [Sui binaries in your PATH](https://docs.sui.io/build/install#binaries) environment variable

:tip: To reliably test DevNet with the latest Sui binaies, re-install them at least weekly.

In addition, to conduct advanced work such as publishing a Move module or making a Move call, also download the Sui source code.
