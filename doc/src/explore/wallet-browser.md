---
title: Sui Wallet
---

This topic describes how to install and use the [Sui Wallet Browser Extension](https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil). You can use the Sui Wallet to create an address, complete transactions, mint NFTs, view or manage assets on the Sui network, and connect with blockchain dApps on Web3.

The early versions of Sui Wallet let you experiment with the Sui network for testing. The Sui network is still in development, and the tokens have no real value. Accounts reset with each deployment of a new version of the network. View the [devnet-updates](https://discord.com/channels/916379725201563759/1004638487078772736) channel in Discord for updates about the network.

To test more advanced features not available in Sui Wallet, see [Sui CLI client](../build/cli-client.md).

## Sui Wallet features

You can use Sui Wallet to:

* Mint NFTs
* Transfer coins and NFTs to another address
* View your coins, tokens, and NFTs
* View recent transactions
* Auto split/merge coins to the exact transfer amount
* Easily access transaction history in the [Sui Explorer](https://explorer.sui.io/)

Note that in the current release, Sui Wallet includes buttons to **Buy**, **Swap**, and **Stake & Earn SUI**. These are placeholders for functionality included in future versions of Sui Wallet.

## Install the Sui Wallet browser extension

To use Sui Wallet you must install a Chrome browser extension. You can use the extension with any browser that supports Chrome extensions from the Chrome Web Store.

1. Open the [Sui Wallet](https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil) page on the Chrome Web Store.
1. Click **Add to Chrome**.
1. Acknowledge the message about permissions for the extension, and then click **Add Extension**.

## Create a new wallet

If you don't yet have a Sui Wallet, create a new one. To import an existing wallet, see [Import an existing Sui Wallet](#import-an-existing-sui-wallet).

1. Open the Sui Wallet extension in your browser and then click **Get Started**.
1. Click **Create a New Wallet**.
1. Enter a password for your wallet.
   This is not a global password for Sui Wallet. It applies only to this installation.
1. Enter the password again to confirm it.
1. Click the checkbox to accept the Terms of Service.
1. Click **Create Wallet**.
1. Copy the Recovery Phrase and store it in a safe location.
1. Click **Open Sui Wallet**.

Sui Wallet prompts you to enter your password when you open it.

If you lose access to your wallet, you can recover it only with the recovery phrase. If you lose the recovery phrase, you lose access to your wallet and any coins or NFTs stored in it.

## Import an existing Sui Wallet

You can use your Sui Wallet on multiple devices and browsers. After you create a Sui Wallet, use the 12-word recovery phrase to import your wallet to a new browser or device.

1. Open the Sui Wallet extension in your browser and then click **Get Started**.
1. Click **Import an Existing Wallet**.
1. Enter your 12-word recovery phrase, and then click **Continue**.
1. Enter a password for your wallet.
   This is not a global password for Sui Wallet. It applies only to this installation.
1. Enter the password again to confirm it.
1. Click **Import**.
1. Click **Open Sui Wallet**.

Sui Wallet prompts you to enter your password when you open it.

## View Sui Wallet details

To view details about your Sui Wallet, including the Account ID, current network, and installed version, click the menu (the three bars) at the top-right corner of the Sui Wallet interface.

## Reset your Sui Wallet password

If you forget the password for your Sui Wallet you can reset it using your 12-word recovery phrase.

1. Click **Forgot password?** on the **Welcome Back** page.
1. Enter your 12-word recovery phrase, and then click **Continue**.
1. Enter a password, then confirm the password.
1. Click **Reset**.

## Lock your Sui Wallet

You can lock your wallet to prevent unauthorized access. You must enter your password to unlock it.

1. Click the menu (the three bars) at the top-right corner of the Sui Wallet interface.
1. Click **Lock Wallet**.

You can also set a timer to automatically lock your wallet after a period of idle time.

1. Click the menu (the three bars) at the top-right corner of the Sui Wallet interface.
1. Click **Account**.
1. In the field under **AUTO-LOCK TIMER**, enter the number of minutes to wait, up to 30, before the wallet locks, and then click **Save**.

Sui Wallet locks automatically if you change tabs in your browser.

## Change the active network

You can change the active network for Sui Wallet. Currently, Sui Wallet supports only Devnet and Local. Use Devnet unless you have a local network for testing. To learn how to create a local network, see [Sui Client CLI](../build/cli-client.md).

1. Click the menu (the three bars) at the top-right corner of the Sui Wallet interface.
1. Click **Network**.
1. Click the network to use.
   A checkmark displays next to the active network.

## Add SUI tokens to your Sui Wallet

When you first open the wallet, you have no coins in it. You can add test SUI coins to your wallet directly from the wallet interface, or you can use the faucet in Discord. You need an active Discord account to access the Sui channels.

**To get SUI test coins from Sui Wallet**
If you have no coins in your wallet, click **Request Devnet SUI tokens** to add coins to your wallet. You can request coins 5 times every 60 minutes. After you request tokens the first time, the button moves to the menu so it doesn't take up space on your wallet interface.

If you don't yet have coins in your wallet, click the menu in the top-right corner, and then click **Request Devnet SUI tokens**.

**To get SUI test coins through Discord**
1. Click **Coins**.
1. Click the small clipboard icon next to your address to copy it.
It's near the top of the wallet and starts with 0x.
1. Open the Sui [devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) channel in Discord.
1. Use the `!faucet` command with your wallet address to request tokens:
   `!faucet 0x6c04ed5110554acf59ff1b535129548dd9a0c741`
   Replace the address in the command with your wallet address.

The channel bot displays a message that starts with "5 test SUI objects are heading to your wallet..."

## View your wallet balance

To view your wallet balance, click **Coins**. The wallet shows your SUI balance and lists the other coins in your wallet, if any.

## Send coins

You can send coins from your wallet to another address.

1. Open the Sui Wallet extension in your browser.
1. Click **Coins** and then click **Send**.
1. In the **Amount** field, enter the number of SUI tokens to send, and then click **Continue**.
1. Enter the recipient's address, then click **Send Coins Now**.

## View recent transaction details

The wallet displays the recent transactions to and from your wallet on the **Activity** tab. Click on any transaction to view transaction details.

## View all transactions in Sui Explorer

You can view all transactions for your address in [Sui Explorer](https://explorer.sui.io/).

To view all of the transactions for your address, click **Apps** and then click **View account on Sui Explorer**.

Sui Explorer opens with the details for your wallet address displayed.

## Mint an example NFT

You can mint an example Sui NFT directly from Sui Wallet.

Click **Apps**, then click **Mint an NFT**. In the current version you can mint only example NFTs.

## Create a new NFT

The [Sui Wallet demo](https://sui-wallet-demo.sui.io/) site lets you create a new NFT on the Sui network using your own image file. To access the site directly from Sui Wallet, click the **Apps** tab, and then click **Sui NFT Mint**. You must have an active wallet to mint NFTs.

To mint a new NFT using the demo site
1. Open the [Sui Wallet demo](https://sui-wallet-demo.sui.io/) site.
1. Click **Connect**.
1. In your Sui Wallet, click **Connect** to connect your wallet with the demo site.
   You may need to enter your wallet password.
1. Enter a **Name** and **Description** for your NFT, and then enter Image URL to the image to use.
1. Click **Create**.
1. Click **Approve** in your wallet to allow the site to add the NFT to your wallet.

After you successfully create a new NFT, you can transfer it to another wallet address. Enter the address to send it to in the **Recipient** field, then click **Transfer**. Click **Approve** in your wallet to allow the transfer.

You can view details for the transactions to create the NFT and then transfer it in [Sui Explorer](https://explorer.sui.io/).

## View your NFTs

Click the **NFTs** tab to view all of the NFTS that you mint, purchase, or receive in your wallet. This includes any NFTs that you obtain from connected apps.  Click on an NFT to view additional details about it, view a larger NFT image, or send the NFT to another address.

## Send an NFT

You can use Sui Wallet to send an NFT to another address.

1. Click **NFTs**.
1. Click on the NFT to send, and then click **Send NFT**.
1. Enter the recipient address then click **Send NFT Now**.
1. Click **Done** to return to the wallet.

## Wallet Playground

You can view and try out some apps that already support Sui Wallet from the Playground on the Apps tab. The apps displayed let you connect your Sui Wallet and use SUI tokens to interact with them, perform transactions, and obtain NFTs that go directly to your connected wallet.

Click on an app to open the site for the app. Follow the guidance on the site to connect your wallet. After you connect your wallet to an app you can view the app on the **Active Connections** view.

## View connected apps

To view the apps with active connections to your wallet, click **Apps**. By default, the **Playground** view displays. Click **Active Connections** to view the connected apps.

To open the site associated with the app, click on the app and then click **View**.

## Disconnect from an app

You can easily disconnect your wallet from a connected app.
1. Click **Apps** and then click **Active Connections**.
1. Click the app to disconnect from your wallet, then click **Disconnect**.

Your wallet immediately disconnects from the app and returns to the **Apps** tab.