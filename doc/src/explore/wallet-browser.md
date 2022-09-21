---
title: Sui Wallet
---

This topic describes how to install and use the [Sui Wallet Browser Extension](https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil). You can use the Sui Wallet to create an address and complete transactions, mint NFTs, view and manage assets on the Sui network, and connect with blockchain dApps on Web3.

The early versions of the Sui Wallet let you experiment with the Sui network for testing. The Sui network is still in development, and the tokens have no real value. Accounts are reset with each deployment of a new version of the network. View the [devnet-updates](https://discord.com/channels/916379725201563759/1004638487078772736) channel in Discord for updates about the network.

To test more advanced features that are not supported in Sui Wallet, see [Sui CLI client](../build/cli-client.md).

## Sui Wallet features

You can use the Sui Wallet to:

   * Mint NFTs
   * Transfer coins and NFTs to another address
   * View your coins, tokens, and NFTs
   * View recent transactions
   * Auto split/merge coins to the exact transfer amount
   * Easily access transaction history in the [Sui Explorer](https://explorer.devnet.sui.io/)

## Install the Sui Wallet Chrome Browser Extension

The Sui Wallet is provided as a Chrome browser extension. You can use the extension with any Chrome-based browser.

   1. Open the [Sui Wallet](https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil) page on the Google Chrome Store.
   1. Click **Add to Chrome**.
   1. Acknowledge the message about permissions for the extension, and then click **Add Extension**.

## Create a new wallet

If you don't yet have a Sui Wallet, create a new one. To import an existing wallet, see [Import an existing Sui Wallet](#import-an-existing-sui-wallet).

   1. Open the Sui Wallet extension in your browser, and then click **Get Started**.
   1. Click **Create new wallet**.
   1. Click the checkbox to accept the Terms of Service.
   1. Click **Create Wallet Now**.
   1. Copy the Recovery passphrase and store it in a safe location.
   1. Click **Done**.

If you lose access to your wallet, you can recover it only with the recovery passphrase. If you lose the passphrase, you lose access to your wallet and any funds or NFTs stored in it.

## Import an existing Sui Wallet

You can use your Sui Wallet on multiple devices and browsers. After you create your first Sui Wallet, use the 12-word recovery passphrase to import your wallet to a new browser or device. 

   1. Open the Sui Wallet extension in your browser, and then click **Get Started**.
   1. Click **Import a wallet**.
   1. Enter your 12-word recovery passphrase.
   1. Click **Import Wallet Now**.

## Add Sui tokens to your Sui Wallet

When you first open the wallet you have no coins in it. You can add tokens to your wallet through Discord. You need an active Discord account to access the Sui channels.

   1. Open the Sui Wallet extension in your browser.
   1. Click the small clipboard icon next to your address to copy it. It's displayed near the top and starts with 0x.
   1. Open the [devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) channel in Discord.
   1. Use the !faucet command with your wallet address to request tokens for your wallet:
   `!faucet 0x6c04ed5110554acf59ff1b535129548dd9a0c741`
   Replace the address in the command with your wallet address.

After you use the command a message that "5 test SUI objects are heading to your wallet" displays. You can then see 250,000 SUI tokens in your wallet.

## View your account balance
To view your account balance, click **Coins**. Your balance is displayed near the top of the wallet.

## Send coins
You can send coins from your wallet to another wallet or account address.

   1. Open the Sui Wallet extension in your browser.
   1. Click **Coins** and then click **Send**.
   1. In the **Amount** field, enter the number of SUI tokens to send.
   1. Click **Continue**.
   1. Enter the address to send the SUI tokens to. Make sure to use the correct address.
   1. Click **Send Coins Now**.




   * Transfer NFTs by clicking **Send** on the _NFT_ tab:
   * View _recent transactions_ by clicking the **Arrow** icon at the top:
   * Sign transactions through a framework connecting Sui wallet to other DApps.
   * Go to the [Sui Explorer](https://explorer.devnet.sui.io/) to view of the current transaction by clicking the external link icon at the bottom right.





## Connected apps

You can view the apps you connected your wallet to on the ---need more info---


