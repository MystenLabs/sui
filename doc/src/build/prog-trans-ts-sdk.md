---
title: Sui Programmable Transaction Blocks with the TS SDK
---

One of Sui’s most powerful core developer primitives is Programmable Transaction Blocks. For conventional blockchains, a transaction is the fundamental unit of execution, and each transaction is typically simplistic and close to the VM execution. On Sui, the fundamental, atomic unit of execution is elevated to the level of a complex, composable sequence of transactions where:

* Any public on-chain Move function across all smart contracts is accessible to the programmable transaction block.
* Typed outputs from earlier on-chain Move calls can be chained as typed inputs to later on-chain Move calls. These types can be arbitrary Sui objects that carry a rich set of attributes and properties. Programmable transaction blocks can be highly heterogeneous. A single block can extract a **Player** object from a smart contract wallet, use it to make a move in a **Game**, then send a **Badge** object won by the move to a multi-game **TrophyCase**, all without publishing any new smart contract code. The natural compositionality of these blocks allow existing contracts to seamlessly interoperate with both old and new code (for example, the **Game** does not have to know/care that the user stores their **Player** in a Multisig wallet or their **Badge** in a **TrophyCase**).
* Chained transactions in a Programmable Transaction block execute and fail atomically. This [example Defi transaction block](https://suiexplorer.com/txblock/94u4E3hLbpbURU7Ja3NRSwk4Xvm22iq9GsTC5MrtLrnN?network=testnet&ref=blog.sui.io) is a programmable transaction block with 12 operations - it performs five swaps across three distinct pools, mutating 20 existing objects and creating seven new ones in the process.
* Each Programmable Transaction block supports up to 1,024 transactions, which enables unbounded expressivity and efficiency. You can use these blocks for homogeneous batching (such as for payments or NFT mints), and heterogeneous chains of single-sender operations as described in the two preceding examples. Both modes leverage Sui's high-speed execution, and allow developers to push already low transaction fees even lower by packing more productive work into a single block.

With the power and convenience of Programmable Transaction blocks, developers on Sui are constructing increasingly sophisticated blocks customized for their applications. Sui’s programmability was highly expressive even before Programmable Transaction blocks. Now, a single execution can perform up to 1,024 heterogeneous operations. On most other blockchains, each of the 1,024 operations would be an individual transaction.

## Get Started

To get started using Programmable Transaction blocks, make sure that you have the latest TypeScript SDK installed.

This example starts by constructing a transaction block to send Sui. If you are familiar with the legacy Sui transaction types, this is similar to a `paySui` transaction. To construct transactions, import the `TransactionBlock` class, and construct it:

```tsx
import { TransactionBlock } from "@mysten/sui.js";
const txb = new TransactionBlock();
```

Using this, you can then add transactions to this transaction block.

```tsx
// Create a new coin with balance 100, based on the coins used as gas payment.
// You can define any balance here.
const [coin] = txb.splitCoins(txb.gas, [txb.pure(100)]);

// Transfer the split coin to a specific address.
txb.transferObjects([coin], txb.pure("0xSomeSuiAddress"));
```

Note that you can attach multiple transactions of the same type to a transaction block as well. For example, to get a list of transfers, and iterate over them to transfer coins to each of them:

```tsx
interface Transfer {
  to: string;
  amount: number;
}

// Procure a list of some Sui transfers to make:
const transfers: Transfer[] = getTransfers();

const txb = new TransactionBlock();

// First, split the gas coin into multiple coins:
const coins = txb.splitCoins(
  txb.gas,
  transfers.map((transfer) => txb.pure(transfer.amount))
);

// Next, create a transfer transaction for each coin:
transfers.forEach((transfer, index) => {
  txb.transferObjects([coins[index]], txb.pure(transfer.to));
});
```

After you have the transaction block defined, you can directly execute it with a signer using `signAndExecuteTransactionBlock`.

```tsx
signer.signAndExecuteTransactionBlock({ transactionBlock: txb });
```

## Inputs and transactions

Programmable Transaction blocks have two key concepts: inputs and transactions.

Inputs are values that are used as arguments to the transactions in the transaction block. Inputs can either be an object reference (either to an owned object, an immutable object, or a shared object), or a pure BCS value (for example, an encoded string used as an argument to a move call).

Transactions are steps of execution in the transaction block. You can also use the result of previous transaction as an argument to future transactions. By combining multiple transactions together, Programmable Transaction blocks provide a flexible way to create complex transactions.

## Constructing inputs

Inputs are how you provide external values to transaction blocks. For example, defining an amount of Sui to transfer, or which object to pass into a Move call, or a shared object. There are currently two ways to define inputs:

- **For objects:** the `txb.object(objectId)` function is used to construct an input that contains an object reference.
- **For pure values:** the `txb.pure(rawValue)` function is used, and returns an input reference that you use in transactions.

## Available transactions

Sui supports following transactions:

- `txb.splitCoins(coin, amounts)` - Creates new coins with the defined amounts, split from the provided coin. Returns the coins so that it can be used in subsequent transactions.
  - Example: `txb.splitCoins(txb.gas, [txb.pure(100), txb.pure(200)])`
- `txb.mergeCoins(destinationCoin, sourceCoins)` - Merges the sourceCoins into the destinationCoin.
  - Example: `txb.mergeCoins(txb.object(coin1), [txb.object(coin2), txb.object(coin3)])`
- `txb.transferObjects(objects, address)` - Transfers a list of objects to the specified address.
  - Example: `txb.transferObjects([txb.object(thing1), txb.object(thing2)], txb.pure(myAddress))`
- `txb.moveCall({ target, arguments, typeArguments  })` - Executes a Move call. Returns whatever the Sui Move call returns.
  - Example: `txb.moveCall({ target: '0x2::devnet_nft::mint', arguments: [txb.pure(name), txb.pure(description), txb.pure(image)] })`
- `txb.makeMoveVec({ type, objects })` - Constructs a vector of objects that can be passed into a `moveCall`. This is required as there’s no way to define a vector as an input.
  - Example: `txb.makeMoveVec({ objects: [txb.object(id1), txb.object(id2)] })`
- `txb.publish(modules, dependencies)` - Publishes a Move package. Returns the upgrade capability object.

## Passing transaction results as arguments

You can use the result of a transaction as an argument in a subsequent transactions. Each transaction method on the transaction builder returns a reference to the transaction result.

```tsx
// Split a coin object off of the gas object:
const [coin] = txb.splitCoins(txb.gas, [txb.pure(100)]);
// Transfer the resulting coin object:
txb.transferObjects([coin], txb.pure(address));
```

When a transaction returns multiple results, you can access the result at a specific index either using destructuring, or array indexes.

```tsx
// Destructuring (preferred, as it gives you logical local names):
const [nft1, nft2] = txb.moveCall({ target: "0x2::nft::mint_many" });
txb.transferObjects([nft1, nft2], txb.pure(address));

// Array indexes:
const mintMany = txb.moveCall({ target: "0x2::nft::mint_many" });
txb.transferObjects([mintMany[0], mintMany[1]], txb.pure(address));
```

## Use the gas coin

With Programmable Transaction blocks, you can use the gas payment coin to construct coins with a set balance using `splitCoin`. This is useful for Sui payments, and avoids the need for up-front coin selection. You can use `txb.gas` to access the gas coin in a transaction block, and it is valid as input for any arguments, as long as it is used [by-reference](../build/programming-with-objects/ch2-using-objects.md#pass-objects-by-reference). Practically speaking, this means you can also add to the gas coin with `mergeCoins` and borrow it for Move functions with `moveCall`.

You can also transfer the gas coin using `transferObjects`, in the event that you want to transfer all of your coin balance to another address.

## Get transaction block bytes

If you need the transaction block bytes, instead of signing or executing the transaction block, you can use the `build` method on the transaction builder itself.

**Important:** You might need to explicitly call `setSender()` on the transaction block to ensure that the `sender` field is populated. This is normally done by the signer before signing the transaction, but will not be done automatically if you’re building the transaction block bytes yourself.

```tsx
const txb = new TransactionBlock();

// ... add some transactions...

await txb.build({ provider });
```

In most cases, building requires your JSON RPC Provider to fully resolve input values.

If you have transaction block bytes, you can also convert them back into a `TransactionBlock` class:

```tsx
const bytes = getTransactionBlockBytesFromSomewhere();
const txb = TransactionBlock.from(bytes);
```

## Building Offline

In the event that you want to build a transaction block offline (i.e. with no `provider` required), you need to fully define all of your input values, and gas configuration (see the following example). For pure values, you can provide a `Uint8Array` which will be used directly in the transaction. For objects, you can use the `Inputs` helper to construct an object reference.

```tsx
import { Inputs } from "@mysten/sui.js";

// For pure values:
txb.pure(pureValueAsBytes);

// For owned or immutable objects:
txb.object(Inputs.ObjectRef({ digest, objectId, version }));

// For shared objects:
txb.object(Inputs.SharedObjectRef({ objectId, initialSharedVersion, mutable }));
```

You can then omit the `provider` object when calling `build` on the transaction. If there is any required data that is missing, this will throw an error.

## Gas Configuration

The new transaction builder comes with default behavior for all gas logic, including automatically setting the gas price, budget, and selecting coins to be used as gas. This behavior can be customized.

### Gas Price

By default, the gas price is set to the reference gas price of the network. You can also explicitly set the gas price of the transaction block by calling `setGasPrice` on the transaction builder.

```tsx
txb.setGasPrice(gasPrice);
```

### Budget

By default, the gas budget is automatically derived by executing a dry-run of the transaction block beforehand. The dry run gas consumption is then used to determine a balance for the transaction. You can override this behavior by explicitly setting a gas budget for the transaction, by calling `setGasBudget` on the transaction builder.

**Note:** The gas budget is represented in Sui, and should take the gas price of the transaction block into account.

```tsx
txb.setGasBudget(gasBudgetAmount);
```

### Gas Payment

By default, the gas payment is automatically determined by the SDK. The SDK selects all of the users coins that are not used as inputs in the transaction block.

The list of coins used as gas payment will be merged down into a single gas coin before executing the transaction block, and all but one of the gas objects will be deleted. The gas coin at the 0-index will be the coin that all others are merged into.

```tsx
// NOTE: You need to ensure that the coins do not overlap with any
// of the input objects for the transaction block.
txb.setGasPayment([coin1, coin2]);
```

### Dapp / Wallet Integration

The Wallet Standard interface has been updated to support the `TransactionBlock` kind directly. All `signTransaction` and `signAndExecuteTransaction` calls from dapps into wallets will be expected to provide a `TransactionBlock` class. This transaction block class can then be serialized and sent to your wallet for execution.

To serialize a transaction block for sending to a wallet, Sui recommends using the `txb.serialize()` function, which returns an opaque string representation of the transaction block that can be passed from the wallet standard dapp context to your wallet. This can then be converted back into a `TransactionBlock` using `TransactionBlock.from()`.

**Important:** You should not build the transaction block from bytes in the dApp code. Using `serialize` instead of `build` allows you to build the transaction block bytes within the wallet itself. This allows the wallet to perform gas logic and coin selection as needed.

```tsx
// Within a dApp
const tx = new TransactionBlock();
wallet.signTransactionBlock({ transactionBlock: tx });

// Your wallet standard code:
function handleSignTransactionBlock(input) {
  sendToWalletContext({ transactionBlock: input.transactionBlock.serialize() });
}

// Within your wallet context:
function handleSignRequest(input) {
  const userTx = TransactionBlock.from(input.transaction);
}
```

## Sponsored transaction blocks

The transaction block builder can support sponsored transaction blocks by using the `onlyTransactionKind` flag when building the transaction block.

```tsx
const txb = new TransactionBlock();

// ... add some transactions...

const kindBytes = await txb.build({ provider, onlyTransactionKind: true });

// Construct a sponsored transaction from the kind bytes:
const sponsoredTxb = TransactionBlock.fromKind(kindBytes);

// You can now set the sponsored transaction data that is required:
sponsoredTxb.setSender(sender);
sponsoredTxb.setGasOwner(sponsor);
sponsoredTxb.setGasPayment(sponsorCoins);
```
