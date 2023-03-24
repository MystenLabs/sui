---
title: Sui Sponsored Transactions
---

A Sui Sponsored transaction is one where a Sui address (the sponsor’s) pays the gas fees for a transaction initialized by another address (the user’s). You can use Sponsored transactions to cover the fees for users on your site or app so that they do not get charged for them. This removes a significant obstacle that web2 users encounter when entering web3, as they often have to purchase tokens to perform a transaction on chain. For example, you could increase conversion rates for gamers by sponsoring their early transactions.

Sponsored transactions also facilitate asset management as you don’t need to maintain multiple accounts with SUI tokens to transfer funds.

You can use Sui Sponsored transactions to:
 * Sponsor (pay gas fees for) a transaction initiated by a user.
 * Sponsor transactions you initiate as the sponsor.
 * Provide a wildcard GasData object to users. The object covers the gas fees for a user transaction. The GasData object covers any fee amount determined for the transaction as long as the budget is sufficient.

## Potential Risks Using Sponsored Transactions

The most significant potential risk when using sponsored transactions is [equivocation](../learn/sui-glossary#equivocation). In some cases under certain conditions, a sponsored transaction can result in all associated owned objects, including gas in a locked state when examined by Sui validators. To avoid double spending, validators lock objects as they validate transactions. An equivocation occurs when an owned object’s pair (`ObjectID`, `SequenceNumber`) is concurrently used in multiple non-finalized transactions.

To equivocate, either the user or the sponsor signs and submits another transaction that attempts to manipulate an owned object in the original transaction. Because only the object owner can use an owned object, only the user and sponsor can cause this condition.

## Create a user-initiated sponsored transaction

A user-initiated sponsored transaction involves the following steps:

 1. A user initializes a `GasLessTransactionData` transaction.
 1. The user sends `GasLessTransactionData` to the sponsor.
 1. The sponsor validates the transaction, constructs `TransactionData` with gas fees, and then signs `TransactionData`.
 1. The sponsor sends the signed `TransactionData` and the sponsor `Signature` back to the user.
 1. The user verifies and then signs `TransactionData` and sends the dual-signed transaction to Sui network through a Full node or the sponsor.

### GasLessTransactionData

`GasLessTransactionData` is basically `TransactionData` without `GasData`. It is not a sui-core data structure, but it is only an interface between user and sponsor.

The following example constructs a `GasLessTransactionData`  object.

```rust
pub struct GasLessTransactionData {
    pub kind: TransactionKind,
    sender: SuiAddress,
    …
}
```

## Create a sponsor-initiated sponsored transaction  

A sponsor-initiated sponsored transaction involves the following steps:
 1. A sponsor constructs a `TransactionData` object that contains the transaction details and associated gas fee data. The sponsor signs it to generate a `Signature` before sending it to a user. You can send the unsigned `TransactionData` via email, SMS, or an application interface.
 1. The user checks the transaction and signs it to generate the second `Signature` for the transaction.
 1. The user submits the dual-signed transaction to a Sui Full node or sponsor to execute it.

You can use a sponsor-initiated sponsored transaction as an advertiser, or to incentivize specific user actions without requiring the user to pay for gas fees.

## Create sponsored transactions using a GasData object

To use a `GasData` object to sponsor the gas fees for a transaction, create a `GasData` object that covers the fees determined for the transaction. This is similar to providing a blank check to a user that can be used only to cover gas fees. The user doesn’t need to know how much the fee is or approve it.

 A sponsor transaction using a `GasData` object involves the following steps:
 1. The sponsor provides a `GasData` object to a user.
 1. The user constructs `TransactionData` and signs it to generate a `Signature`.
 1. The user sends the `TransactionData` and the `Signature` to the sponsor.
 1. The sponsor confirms the `TransactionData` and then signs it.
 1. The sponsor submits the dual-signed `TransactionData` to a Full node to execute the transaction.

## Create a Sui Gas Station

Anyone can set up and operate a Sui Gas Station to sponsor user transactions. You can customize a Sui Gas Station to support the specific user-facing functionality you need. Here are some example use cases for a Sui Gas Station:
 * Monitor real-time gas prices on the network to determine the gas price that the station provides.
 * Track usage of gas provided to users on the network.
 * Gas pool management, such as using specific gas objects to minimize costs or reduce the risk of a large amount of locked objects that remain illiquid while locked.

### Sui Gas Station Risk Mitigation

Sui supports the following mechanism to mitigate the potential risk associated with sponsored transactions.
Authorization & Rate Limiting
Depending on the nature of the Gas Station, sponsors can apply different authorization rules to avoid being spammed by bad actors. Possible policies include:
 * Rate limit gas request per account or per IP address
 * Only accept requests with a valid authorization header, which has separate rate limits

### Abuse Detection

For all gas objects that the sponsor gives out, track if users ever try to equivocate and lock objects. If such behavior is detected, block list this user or requestor accordingly.

## Code examples to create a Sui Gas Station

The code examples in this section demonstrate how to implement a Sui Gas Station that supports each of the sponsored transaction types described previously in this topic.

### User-initiated sponsored transactions

Use the API endpoint to receive `GaslessTransaction` transactions and return a sole-signed `SenderSignedData` object.

```rust
pub fn request_gas_and_signature(gasless_tx: GaslessTransaction) -> Result<SenderSignedData, Error>;
```

### Sponsored transactions with GasData objects

Use the API endpoint to receive sole-signed `SenderSignedData` and return the result of the transaction.

```rust
pub fn submit_sole_signed_transaction(sole_signed_data: SenderSignedData) -> Result<(Transaction, CertifiedTransactionEffects), Error>;
```

Alternatively, use the API endpoint to return a GasData object.

```rust
pub fn request_gas(/*requirement data*/) -> Result<GasData, Error>;
```

### User and Sponsor-initiated transaction.

Use the API endpoint to receive dual-signed SenderSignedData and return the result of the transaction.

```rust
pub fn submit_dual_signed_transaction(dual_signed_data: SenderSignedData) -> Result<(Transaction, CertifiedTransactionEffects), Error>;
```

For user and sponsor-initiated transactions, users can submit the dual-signed transaction via either a sponsor or a full node.

## Sponsored Transaction Data Structure

The following code block describes the [`TransactionData`](https://github.com/MystenLabs/sui/blob/main/crates/sui-types/src/messages.rs#L999) structure for sponsored transactions and [GasObject](https://github.com/MystenLabs/sui/blob/main/crates/sui-types/src/messages.rs#L982). You can view the [source code](https://github.com/MystenLabs/sui/blob/main/crates/sui-types/src/messages.rs) in the Sui GitHub repository.

**`TransactionData` Structure**
```rust
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct TransactionData {
   pub kind: TransactionKind,
   pub sender: SuiAddress,
   pub gas_data: GasData,
}
```

**`GasData` Structure**
```rust
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct GasData {
   pub payment: ObjectRef,
   pub owner: SuiAddress,
   pub price: u64,
   pub budget: u64,
}
```

To learn more about Transaction in Sui, see [Transactions](../learn/transactions.md).





