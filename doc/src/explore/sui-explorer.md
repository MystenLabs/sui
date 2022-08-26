---
title: Using the Sui Explorer
---

The [Sui Explorer](https://explorer.devnet.sui.io/) lets you view data about transactions and activity on the Sui network. In addition to viewing activity on the network, you can use the Explorer to:
 * View up-to-date information about the activity and metrics on the Sui network.
 * Look up, verify, and track your assets and contracts.
 * Utilize fast, reliable, and transparent debugging and auditing data to help identify and resolve issues. 
 * Get go-to-definition support for all smart contracts, referred to as packages in Sui.
 * View validators and geographic locations of full nodes that are currently active.


## Choose a network
When you start Sui Explorer, it displays the transactions for the Devnet network by default. You can also use the Explorer to view data for a local network running in your environment or a custom RPC endpoint URL. Use the drop-down menu at the top-right of the page to choose a different network.


## Finding your transaction
You can search for the transactions using an address, object ID, or transaction ID. For example, you can search for your wallet address to confirm a transaction or view additional details about a transaction you’ve approved. See [Sui Wallet](../explore/wallet-browser.md) to learn how to create a wallet.  

**To search for a transaction made using the Sui Wallet**
1. Open your Sui Wallet.
1. Click the clipboard icon to copy your wallet address.
1. Open the Sui Explorer. Select the **Coins** tab if it is not currently selected.
1. In the search field, paste your wallet address and then press **Enter**.

The Explorer displays the **Address** details page for your wallet address. You should see the same transaction in Explorer that you see in your wallet history. Click on a transaction to view the details for it.

## Explorer start page
When you open the Sui Explorer, the page displays the transactions and top validators on the network. The **Transactions** table lists the transactions on the network with the most recent transaction first.

The **Top Validators** table lists the top validators on the network and includes a map showing the geographic locations of all nodes on the network.

Click **More Transactions** to open the **Transactions** page and view all of the transactions on the network.

## Transactions page
The **Transactions** page lists all transactions on the network. You can display 20, 40, or 60 rows of transactions per page. Use the drop-down near the bottom-right corner of the page to change the number of rows displayed per page. Use the page selector icons at the bottom of the page to view more transactions.

### Transaction table fields
The **Transactions** table on the page includes the following columns:
 * **Time** - displays the time at which the transaction occurred.
 * **Type** - the type of transaction, one of Call, TransferSui, TransferObject, or Publish.
     * **Call** - This is an API request … For Call transactions, the table includes only the sender address. 
     * **TransferSui** - Indicates that the transaction transferred Sui from one address to another.
     * **TransferObject** - Indicates a transaction to transfer an object to a different address.
     * **Publish** - Indicates a transaction to publish a package.
     * **Batched** - Indicates a batch of transactions.
 * **Transaction ID** - The unique identifier for the transaction. You can click the clipboard icon to copy the ID. Click a value in the **Transaction ID** column to display the details about the transaction.
 * **Addresses** - Displays the addresses of the sender and receivers for the transaction. You can click on an address for additional details and transactions made using the address.
 * **Amount** - Displays the number of coins and coin type used for the transaction.
 * **Gas** - Shows the amount of Sui used to pay for the gas required to complete the transaction.

You can click on a value in the **Transaction ID** or **Addresses** column to open a details page for the transaction or address that you click on. When you click a transaction ID, the page that opens depends on the type of transaction. Sui Eplorer provides the following detail pages:
 * [Transaction details](#transaction-details-pages) for each transaction type
     * TransferSui transaction details
     * TransferObject transaction details
     * Call transaction details
     * Publish transaction details
     * Batch transaction details
 * [Address details](#address-details-page)
 * [Object details](#object-details-page)

## Transaction details pages
When you click a **Transaction ID**, a details page opens. The page title reflects the transaction type, and the fields displayed vary depending on the transaction type. If you don’t see one of the fields, it is because it is not available for the selected transaction type. For example, a TransferSui transaction does not include an **Events** tab.

The transaction details pages include the following tabs:
 * **Details** - Provides additional details about the transaction.
 * **Events** - Displays the events associated with the transaction.
 * **Signatures** - Lists the signatures from validators for the transaction.

The **Details** tab includes the following fields:
 * **Package Details** - Displayed only for Call transactions. Displays the Package ID associated with the transaction and the following fields:
     * Module - The module used for the transaction.
     * Function - The Function called for the transaction.
     * Argument - Any arguments included with the function.
 * **Updated** - The object ID for the object the transaction updated. Click the ID to view the [Object details](#object-details-page) page for more details.
 * **Created** - The object ID for the object this transaction created. Click the ID to view the [Object details](#object-details-page) page for more details.
 * **Amount** - The number and type of coins transferred for the transaction.
 * **Sender** - Displayed only for Publish transactions. The address of the sender of the transaction.
 * **Sender & Recipients** - The addresses associated with the transaction. The first value is the sender's address, and the address next to the green checkmark is the recipient's address. When there are multiple recipients, the field includes multiple addresses.
 * **Modules** - Shows the module code used to create and execute the transaction.
 * **Gas and storage fees** - Details about the gas and fees for the transaction. 
The value for Gas Payment is the object ID for the coin object used for the transaction.
 * **Gas Fees** - The number of gas units used for the transaction. 
 * **Gas Budget** - The maximum number of gas units allowed for the transaction.

The **Events** tab lists the events the transaction generated and the details about each event. TransferSui transactions do not include events.

The **Signatures** tab includes the following fields:
 * **Transaction Signatures** - The signature or signatures for the transaction.
 * **Validator Signatures** - The signatures from the validators that validated the transaction.

## Object details page
When you click on an object ID displayed on a transaction details page it opens a page that displays the details for the object, such as the transactions associated with the object. The page includes the following details:

 * **Description** includes the following fields:
     * **Type** - The type of the object, such as coin.
     * **Object ID** - The ID of the object. 	
     * **Last Transaction ID** - The ID of the most recent transaction associated with the object.
     * **Version** - The version of the object. 
     * **Owner** - The address of the owner of the object.
 * **Properties** - Displays details such as the coin balance for the object.
 * **Child Objects** - Displays the objects that this object owns.
**Transactions** - Displays the same information as the **Transactions** page, but is limited to the transactions associated with the object.

## Address details page
The **Address** details page lets you view details about a specific address, including assets owned by the address and transactions that interacted with the address.

The **Address** details page includes the following fields:
 * **Owned objects** - Displays the objects owned by the address, such as coins.
 * **Coins** - List of tokens owned and their aggregated balance by coin type. Click on an entry to view additional details about individual coin objects.
 * **NFTs** - List of NFTs owned by the address. Click an ID to view the object details page for the NFT.
 * **Transactions** - Click to view more detailed information about each transaction.

