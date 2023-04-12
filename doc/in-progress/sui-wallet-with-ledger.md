---
title: Use Sui Wallet with a Ledger Wallet
---

This guide describes how to use Sui Wallet with your Ledger device. When you connect your Ledger device, you can add up to 10 accounts from your device to Sui Wallet. You can then use the account in Sui Wallet just like any other account, but with the added protection of storing your private keys in a cold storage wallet. This gives you complete control of the private keys for your digital assets. To use your device with Sui Wallet, you just need to install the Sui app on the device, add one or more accounts from the device to Sui Wallet, and then authorize the connection to Sui Wallet when you use the account to perform transactions on the Sui network.

The sections in this guide include:
* [Requirements](#requirements)
* [Install the Sui app on your Ledger device](#install-the-sui-app-on-your-ledger-device)
* [Import accounts from your device to Sui Wallet](#import-accounts-from-your-device-to-sui-wallet)
* [View account balance](#view-your-account-balance-and-assets)
* [Receive digital assets using Sui Wallet](#receive-digital-assets-using-sui-wallet)
* [Send digital assets using Sui Wallet](#send-digital-assets-using-sui-wallet)
* [Support for Sui Wallet](#support-for-sui-wallet)

## Requirements

Before you connect your device to Sui Wallet:
* [Install the latest version of Sui Wallet](https://docs.sui.io/devnet/explore/wallet-browser#install-the-sui-wallet-browser-extension) from the Chrome Web Store.
* [Set up and configure](https://support.ledger.com/hc/en-us/sections/4404369606801-Getting-Started?docs=true) your Ledger device, including updating to the latest firmware.
* Install [Ledger Live](https://www.ledger.com/ledger-live) and confirm that you can connect your device to Ledger Live.

## Install the Sui app on your Ledger device

You can install the Sui app on your Ledger device from the App Catalog in Ledger Live.

1. Unlock your device.
1. Open Ledger Live and click **My Ledger** in the left panel.
1. Press both buttons on the device to allow the secure connection with Ledger.
1. In the **App catalog**, search for **Sui**.
1. Click **Install** to install the Sui app on your device.

Your device shows a message that indicates the installation progress.
 
## Import accounts from your device to Sui Wallet

You need to connect your device to Sui Wallet so you can import accounts.

1. Unlock your Ledger device if not unlocked.
1. Select the Sui app on the device and then press both buttons to start it. The screen should show Sui with the version number, such as 1.0.0.
1. Quit Ledger Live if it is still running.
1. Open Sui Wallet and enter your password.
1. Click the Settings menu (the three bars at the top-right corner), then click **Accounts**.
1. Click **Connect Ledger Wallet**.
   You might need to scroll to see the button if you have a lot of accounts in your wallet.
1. Unlock your device before you perform the next step.
1. On the **Connect Ledger Wallet** screen, click **Continue**.
1. Select your device when prompted and then click **Connect**.
1. When connected, choose the account or accounts to import to Sui Wallet, and then click **Unlock**.

Sui Wallet displays the accounts you selected on the **Accounts** screen. The accounts imported from your Ledger device show *LEDGER* next to them. When you use Sui Wallet, you can select the account to use when you connect to a site or perform a transaction.

##  View your account balance and assets

You can view the balance for an account in Sui Wallet on the **Coins** tab. The coins and SUI stake displayed are for only the selected account address. To view the balance for a different account address, select the account from the drop-down list near the top of the wallet.

To view the coins and tokens in your account that you imported from your Ledger device, select an account that displays *LEDGER* next to it. To view the NFTs for the account, click the **NFTs** tab in Sui Wallet.

## Receive digital assets using Sui Wallet

To receive cryptocurrency or other digital assets such as NFTs in Sui Wallet, select the account to receive the assets on the **Coins** tab, then click the copy icon to copy the address.

Before you use an account from your Ledger device to receive an asset, confirm that the account is connected to your device.

**To receive digital assets to your Ledger account using Sui Wallet** 
1. Open Sui Wallet and click the Settings menu (the three bars displayed in the top-right corner).
1. Click **Accounts**.
1. Click the down arrow next to the Ledger account to receive the asset, then click **Verify Ledger connection**. Make sure that the display shows **Ledger is connected** before you initiate the transaction to receive an asset.
1. Send the digital asset to the address for the Ledger account.

## Send digital assets using Sui Wallet

You can use Sui Wallet to send a digital asset from your Ledger account.

1. Open Sui Wallet and click the Settings menu (the three bars displayed in the top-right corner).
1. Click **Accounts**.
1. Click the down arrow next to the Ledger account to receive the asset, then click **Verify Ledger connection**. Make sure that the display shows **Ledger is connected** before you initiate the transaction to send an asset.
1. Click the **Coins** tab in Sui Wallet, then click **Send**.
1. Enter the amount of SUI to send and the address of the recipient, then click **Review**.
1. Review the details of the transaction to confirm that they are accurate. If they are, click **Send now**. If not, click **Back** to make changes, or click **X** to cancel.
1. Your Ledger device displays **Transfer SUI**. Press the Right button on the device to display the following details. Press it twice to display the full addresses used. You should verify that the addresses are correct before you finalize the transaction. Confirm that the address on the device matches the address displayed in Sui Wallet.
   * The address the transaction is from.
   * The recipient address.
   * The amount to send.
   * Sign transaction.
   * Confirm
1. Press both buttons while it displays **Confirm** to approve the transaction.
   **Note:** Press the Right button one more time to display **Reject**, then press both buttons to reject it.

Sui Wallet displays the result of the transaction.

## Stake SUI using Sui Wallet

You can use Sui Wallet to stake the SUI in your Ledger account just like using any other account in Sui Wallet. However, when you use an account from your Ledger device to stake SUI, the device doesn't recognize the transaction type. As a result, you can't directly approve (sign) the transaction. Instead, you can enable blind signing on your Ledger device, and then approve the blind signing of the transaction just like approving a direct transaction.

1. Unlock you Ledger device and start the Sui app.
1. Enable Blind signing by pressing the Right button until **Blind signing** displays.
1. Press both buttons to select the setting, then press both buttons again to enable blind signing.
1. Press the Right button again to display **Back**, then press both buttons.
1. Open Sui Wallet and select the account to use. Accounts from your Ledger device show _LEDGER_ next to them. The selected account shows a green check mark next to it.
1. Click **Stake & earn SUI**.
1. Select a validator to stake with. You can view more details about validators on [Sui Explorer](https://explorer.sui.io/validators). 
1. Click **Select Amount** to enter the amount of SUI to stake.
1. Enter an amount of at least 1.0 SUI, then click **Stake Now**.
   If you didn't enable blind signing, your Ledger device displays a warning that the transaction is not recognized and instructs you to enable Blind signing. 
1. Press the Right button to view the following details:
   * The transaction hash
   * **Blind sign transaction**
   * **Confirm**
1. Press both buttons while it displays **Confirm** to confirm the stake transaction.
   **Note:** Press the Right button one more time display **Reject**, then press both buttons to reject the transaction.
1. The device displays **Working** and then completes the transaction. Sui Wallet displays the result of the transaction.

## Support for Sui Wallet

To get help with Sui Wallet 
