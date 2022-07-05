---
title: Using the Sui Wallet Browser Extension
---

Welcome to the [Sui Wallet Browser Chrome Extension](https://chrome.google.com/webstore/detail/sui-wallet/albddfdbohgeonpapellnjadnddglhgn?hl=en&authuser=0) covering its installation and use. The Sui Wallet Browser Extension acts as your portal to the Web3 world.

The Sui Wallet Browser Extension lets you:

* Create, import, and persistently store the mnemonics and the derived private key
* Transfer coins
* See owned fungible tokens and NFTs
* Display recent transactions
* Go directly to the successful/failed transaction in the Sui Explorer

Note, the wallet will auto split/merge coins if the address does not have a Coin object with the exact transfer amount.

Initially, the Sui Wallet Browser Extension is aimed at Sui developers for testing purposes. As such, the tokens are of no value (just like the rest of [DevNet](https://github.com/MystenLabs/sui/blob/main/doc/src/explore/devnet.md)) and will disappear each time we reset the network. In time, the Sui Wallet Browser Extension will be production ready for real tokens.

This browser extension is a pared-down version of the [Sui Wallet command line interface (CLI)](https://github.com/MystenLabs/sui/blob/main/doc/src/build/wallet.md) that provides greater ease of use for the most commonly used features. If you need more advanced features, such as merge/split coins or make arbitrary [Move](https://github.com/MystenLabs/sui/blob/main/doc/src/build/move.md) calls, instead use the [Wallet CLI](https://github.com/MystenLabs/sui/blob/main/doc/src/build/wallet.md).


## Install

To install the [Sui Wallet Browser Extension](https://chrome.google.com/webstore/detail/sui-wallet/albddfdbohgeonpapellnjadnddglhgn?hl=en&authuser=0):

1. Visit its [link in the Chrome Webstore](https://chrome.google.com/webstore/detail/sui-wallet/albddfdbohgeonpapellnjadnddglhgn?hl=en&authuser=0).
1. Click **Install**.
1. Optionally, [pin the extension](https://www.howtogeek.com/683099/how-to-pin-and-unpin-extensions-from-the-chrome-toolbar/) to add it to your toolbar for easy access.

## Startup

1. Open the extension and click **Get Started**:

img

2. Click **Create new wallet**:


3. Accept the terms of service and click **Create**:


5. View and capture the distinct Backup Recovery Passphrase (mnemonic) for the new wallet.

7. Click **Done**.


## Configure

In the Wallet home page, you will see the message _No Tokens Found_:

From the _Active Account_, copy your **address**:


Join [Discord](https://discord.gg/sui) If you haven’t already.

Request tokens in the [#devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) channel:

Using the syntax:
`!faucet &lt;YOUR_ADDRESS>` \
For example: \
`!faucet 0xd72c2c90ed9d923cb0ed2ca91db5be9e1c9b5ccb`

A bot on the channel will distribute tokens to you automatically.

Optionally, confirm the transaction in Sui Explorer:


## Use

The Sui Wallet Browser Extension lets you:

* See your account balance by clicking the **Tokens ($)** icon.
* Send coins by clicking **Send** in the _Tokens_ tab.
* Transfer NFTs by clicking **Send NFT** on the _NFT_ tab.
* Go to the [Sui Explorer](https://explorer.devnet.sui.io/) view of the current transaction by clicking the external link icon at the bottom right.
* See _recent transactions_ by clicking the **Arrow** icon at the top.
* From the **Settings (gear)** menu, you may:
    * View your account on the Sui Explorer
    * Mint Demo NFTs
    * See the Sui terms of service
    * Log out of the Wallet
