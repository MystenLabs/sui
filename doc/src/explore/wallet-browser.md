---
title: Using the Sui Wallet Browser Extension
---

Welcome to the [Sui Wallet Browser Chrome Extension](https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil). The Sui Wallet Browser Extension acts as your portal to the Web3 world. Follow this guide to install and use the extension.

## Purpose

Initially, the Sui Wallet Browser Extension is aimed at Sui developers for testing purposes. As such, the tokens are of no value (just like the rest of [DevNet](../explore/devnet.md)) and will disappear each time we reset the network. In time, the Sui Wallet Browser Extension will be production ready for real tokens.

This browser extension is a pared-down version of the [Sui CLI client](../build/cli-client.md) that provides greater ease of use for the most commonly used features. If you need more advanced features, such as merging/splitting coins and making arbitrary [Move](../build/move/index.md) calls, instead use the [Sui CLI client](../build/cli-client.md).

## Features

The Sui Wallet Browser Extension offers these features:

* Create, import, and persistently store the backup recovery passphrases (mnemonics) and the derived private key
* Create NFTs
* Transfer coins
* See owned fungible tokens and NFTs
* Display recent transactions
* Auto split/merge coins if the address does not have a Coin object with the exact transfer amount
* Go directly to the successful/failed transaction in the [Sui Explorer](https://explorer.devnet.sui.io/)
* A demonstration [NFT dApp](https://github.com/MystenLabs/sui/tree/main/wallet/examples/demo-nft-dapp) available [in the Cloud](http://sui-wallet-demo.s3-website-us-east-1.amazonaws.com/)

See [Demos](#demos) for depictions of these features in play and [Use](#use) to find these features in navigation.

## Demos

The following animated GIFs walk you through some of the most common workflows in the Sui Wallet Browser Extension.

### Set up Wallet

Install and configure the Sui Wallet Browser Extension (covered in detail starting with [Install](#install)):

![Set up Wallet](../../static/wallet_0.0.2/set_up_wallet.gif "Set up Wallet")
*Set up the Sui Wallet Browser Extension*

### Create NFT

From a demo decentralized site, such as our demonstration [NFT dApp](https://github.com/MystenLabs/sui/tree/main/wallet/examples/demo-nft-dapp) available [in the Cloud](http://sui-wallet-demo.s3-website-us-east-1.amazonaws.com/), you can connect to your wallet and create a custom NFT:

![Create NFT](../../static/wallet_0.0.2/create_nft.gif "Create NFT")
*Create an NFT in Sui Wallet by connecting to an external site*

### Transfer NFT

Transfer your NFT to another address using the Sui Wallet Browser Extension:

![Transfer NFT](../../static/wallet_0.0.2/transfer_nft.gif "Transfer NFT")
*Transfer your NFT to another address*

### Transfer token

Transfer your token to another address on the Sui network using the Sui Wallet Browser Extension:

![Transfer token](../../static/wallet_0.0.2/transfer_token.gif "Transfer token")
*Transfer tokens to another address*

### View transaction history

View your recent transactions and visit [Sui Explorer](https://explorer.devnet.sui.io/), where you can see more details about the corresponding transaction:

![Transaction history and settings](../../static/wallet_0.0.2/txn_history.gif "Transaction history and settings")
*Under the *Settings* tab, view your account on Sui Explorer*

## Install

To install the Sui Wallet Browser Extension:
1. Visit its [link in the Chrome Webstore](https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil).
1. Click **Install**.
1. Optionally, [pin the extension](https://www.howtogeek.com/683099/how-to-pin-and-unpin-extensions-from-the-chrome-toolbar/) to add it to your toolbar for easy access.

## Start up

To begin using the Sui Wallet Browser Extension:
1. Open the extension and click **Get Started**:
   ![Start up Sui Wallet](../../static/Sui-wallet-get-started.png "Start up Sui Wallet")
   *Start up Sui Wallet Browser Extension*
1. Click **Create new wallet**:
   ![Create new Sui Wallet](../../static/Sui-wallet-new-account.png "Create new Sui Wallet")
   *Create new wallet with Sui Wallet Browser Extension*
1. Accept the terms of service and click **Create**:
   ![Accept the terms of service for Sui Wallet](../../static/Sui-wallet-ToS.png "Accept ToS")
   *Accept the terms of service for Sui Wallet Browser Extension*
1. View and capture the distinct mnemonic for the new wallet.
1. Click **Done**.

## Configure

In the Wallet home page, you will see the message _No Tokens Found_:
![No tokens found](../../static/Sui-wallet-no-tokens.png "[No tokens found")
*Time to populate your wallet*

To finish setting up the Sui Wallet Browser Extension for testing:
1. From the _Active Account_ in your wallet, copy your **address**:
   ![Copy address from Sui Wallet](../../static/Sui-wallet-copy-address.png "Copy address")
   *Copy your address from the Sui Wallet Browser Extension*
1. Join [Discord](https://discord.gg/sui) If you haven’t already.
1. Request tokens in the [#devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130)
   channel per the [SUI tokens](../build/install.md#sui-tokens) install documentation.
1. Optionally, confirm the transaction in Sui Explorer:
   ![See transfer in Sui Explorer](../../static/Sui-explorer-token-transfer.png "See Sui Explorer")
   *See transfer in Sui Explorer*

## Use

The Sui Wallet Browser Extension lets you:

* See your account balance by clicking the **Tokens ($)** icon:
   ![See your account balance](../../static/tokens.png "See tokens")
   *See your account balance in the Sui Wallet Browser Extension*
* Send coins by clicking **Send** in the _Tokens_ tab:
   ![Send tokens](../../static/token-transfer.png "Send tokens")
   *Send tokens with the Sui Wallet Browser Extension*
* Transfer NFTs by clicking **Send** on the _NFT_ tab:
   ![Transfer NFTs](../../static/NFT-transfer.png "Send tokens")
   *Send NFTs with the Sui Wallet Browser Extension*
* View _recent transactions_ by clicking the **Arrow** icon at the top:
   ![View recent transactions](../../static/txn-history.png "View recent transactions")
   *View recent transactions in the Sui Wallet Browser Extension*
* Sign transactions through a framework connecting Sui wallet to other DApps:
   ![Sign transactions](../../static/txn-signing.png "View recent transactions")
   *Sign transactions in the Sui Wallet Browser Extension*
* From the **Settings (gear)** menu, you may:
    * View your account on the Sui Explorer
    * Mint Demo NFTs
    * See the Sui terms of service
    * Log out of the Wallet
   ![Access settings](../../static/settings.png "Access wallet settings")
   *Access settings for the Sui Wallet Browser Extension*
* Go to the [Sui Explorer](https://explorer.devnet.sui.io/) view of the current transaction by clicking the external link icon at the bottom right.

## Contribute

If you want to experiment with and contribute to the Sui Wallet Browser Extension, you can find its source and README at:
https://github.com/MystenLabs/sui/tree/main/wallet 
