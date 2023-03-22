---
title: Sui Programmable Transaction with the TS SDK
---

In Sui, all user-initiated transactions are called Programmable Transactions.

## Get Started

First, make sure that you have the latest TypeScript SDK installed.

We’ll start by constructing a transaction to send Sui. If you are familiar with the legacy transaction types, this is similar to a `paySui` transaction. You can start constructing transactions by importing the `Transaction` class, and constructing it:

```tsx
import { Transaction } from "@mysten/sui.js";
const tx = new Transaction();
```

Using this, you can then add commands to this transaction.

```tsx
// Create a new coin with balance 100, based on the coins used as gas payment.
// You can define any balance here.
const [coin] = tx.splitCoins(tx.gas, [tx.pure(100)]);

// Transfer the split coin to a specific address.
tx.transferObjects([coin], tx.pure("0xSomeSuiAddress"));
```

Note that you can attach multiple of the same command type to a transaction as well. For example, we can get a list of transfers, and iterate over them to transfer coins to each of them:

```tsx
interface Transfer {
  to: string;
  amount: number;
}

// Procure a list of some Sui transfers we need to make:
const transfers: Transfer[] = getTransfers();

const tx = new Transaction();

// First, split the gas coin into multiple coins:
const coins = tx.splitCoins(
  tx.gas,
  transfers.map((transfer) => tx.pure(transfer.amount))
);

// Next, create a transfer command for each coin:
transfers.forEach((transfer, index) => {
  tx.transferObjects([coins[index]], tx.pure(transfer.to));
});
```

After you have the transaction defined, you can directly execute it with a signer using `signAndExecuteTransaction`.

```tsx
signer.signAndExecuteTransaction({ transaction: tx });
```

## Inputs and commands

Programmable Transactions have two key concepts: inputs and commands.

Inputs are values that are used as arguments to the commands in the transaction. Inputs can either be an object reference (either to an owned object, an immutable object, or a shared object), or a pure BCS value (for example, an encoded string used as an argument to a move call).

Commands are steps of execution in the transaction. You can also use the result of previous commands as arguments to a future command. By combining multiple commands together, programmable transactions provide a flexible way to create complex transactions.

## Constructing inputs

Inputs are how you provide external values to transactions. For example, defining an amount of Sui to transfer, or which object to pass into a move call, or a shared object. There are currently two ways to define inputs:

- **For objects:** the `tx.object(objectId)` function is used to construct an input that contains an object reference.
- **For pure values:** the `tx.pure(rawValue)` function is used, and returns an input reference that you use in commands.

## Available commands

The following commands are available:

- `tx.splitCoins(coin, amounts)` - Creates new coins with the defined amounts, split from the provided coin. Returns the coins so that it can be used in subsequent commands.
  - Example: `tx.splitCoins(tx.gas, [tx.pure(100), tx.pure(200)])`
- `tx.mergeCoins(destinationCoin, sourceCoins)` - Merges the sourceCoins into the destinationCoin.
  - Example: `tx.mergeCoins(tx.object(coin1), [tx.object(coin2), tx.object(coin3)])`
- `tx.transferObjects(objects, address)` - Transfers a list of objects to the specified address.
  - Example: `tx.transferObjects([tx.object(thing1), tx.object(thing2)], tx.pure(myAddress))`
- `tx.moveCall({ target, arguments, typeArguments  })` - Executes a move call. Returns whatever the Sui Move call returns.
  - Example: `tx.moveCall({ target: '0x2::devnet_nft::mint', arguments: [tx.pure(name), tx.pure(description), tx.pure(image)] })`
- `tx.makeMoveVec({ type, objects })` - Constructs a vector of objects that can be passed into a `moveCall`. This is required as there’s no way to define a vector as an input.
  - Example: `tx.makeMoveVec({ objects: [tx.object(id1), tx.object(id2)] })`
- `tx.publish(modules, dependencies)` - Publishes a Move package. Returns the upgrade capability object.

## Passing command results as arguments

You can use the result of a command as an argument in a subsequent command. Each command method on the transaction builder returns a reference to the command result.

```tsx
// Split a coin object off of the gas object:
const [coin] = tx.splitCoins(tx.gas, [tx.pure(100)]);
// Transfer the resulting coin object:
tx.transferObjects([coin], tx.pure(address));
```

When a command returns multiple results, you can access the result at a specific index either using destructuring, or array indexes.

```tsx
// Destructuring (preferred, as it gives you logical local names):
const [nft1, nft2] = tx.moveCall({ target: "0x2::nft::mint_many" });
tx.transferObjects([nft1, nft2], tx.pure(address));

// Array indexes:
const mintMany = tx.moveCall({ target: "0x2::nft::mint_many" });
tx.transferObjects([mintMany[0], mintMany[1]], tx.pure(address));
```

## Use the gas coin

With Programmable Transactions, you’re able to use the gas payment coin to construct coins with a set balance using `splitCoin`. This is useful for Sui payments, and avoids the need for up-front coin selection. You can use `tx.gas` to access the gas coin in a transaction, and it is valid as input for any arguments, as long as it is used [by-reference](../build/programming-with-objects/ch2-using-objects.md#pass-objects-by-reference). Practically speaking, this means you can also add to the gas coin with `mergeCoins` and borrow it for Move functions with `moveCall`.

You can also transfer the gas coin using `transferObjects`, in the event that you want to transfer all of your coin balance to another address.

## Get Transaction Bytes

If you need the transaction bytes, instead of signing or executing the transaction, you can use the `build` method on the transaction builder itself.

**Important:** You might need to explicitly call `setSender()` on the transaction to ensure that the `sender` field has been populated. This is normally done by the signer before signing the transaction, but will not be done automatically if you’re building the transaction bytes yourself.

```tsx
const tx = new Transaction();

// ... add some commands...

await tx.build({ provider });
```

In most cases, building requires your JSON RPC Provider to fully resolve input values.

If you have transaction bytes, you can also convert them back into a `Transaction` class:

```tsx
const bytes = getTransactionBytesFromSomewhere();
const tx = Transaction.from(bytes);
```

## Building Offline

In the event that you want to build a transaction offline (i.e. with no `provider` required), you need to fully define all of your input values, and gas configuration (see the following example). For pure values, you can provide a `Uint8Array` which will be used directly in the transaction. For objects, you can use the `Inputs` helper to construct an object reference.

```tsx
import { Inputs } from "@mysten/sui.js";

// For pure values:
tx.pure(pureValueAsBytes);

// For owned or immutable objects:
tx.object(Inputs.ObjectRef({ digest, objectId, version }));

// For shared objects:
tx.object(Inputs.SharedObjectRef({ objectId, initialSharedVersion, mutable }));
```

You can then omit the `provider` object when calling `build` on the transaction. If there is any required data that is missing, this will throw an error.

## Gas Configuration

The new transaction builder comes with default behavior for all gas logic, including automatically setting the gas price, budget, and selecting coins to be used as gas. This behavior can be customized.

### Gas Price

By default, the gas price is set to the reference gas price of the network. You can also explicitly set the gas price of the transaction by calling `setGasPrice` on the transaction builder.

```tsx
tx.setGasPrice(gasPrice);
```

### Budget

By default, the gas budget is automatically derived by executing a dry-run of the transaction beforehand. The dry run gas consumption is then used to determine a balance for the transaction. You can override this behavior by explicitly setting a gas budget for the transaction, by calling `setGasBudget` on the transaction builder.

**Note:** The gas budget is represented in Sui, and should take the gas price of the transaction into account.

```tsx
tx.setGasBudget(gasBudgetAmount);
```

### Gas Payment

By default, the gas payment is automatically determined by the SDK. The SDK will select all of the users coins that are not used as inputs in the transaction.

The list of coins used as gas payment will be merged down into a single gas coin before executing the transaction, and all but one of the gas objects will be deleted. The gas coin at the 0-index will be the coin that all others are merged into.

```tsx
// NOTE: You need to ensure that the coins do not overlap with any
// of the input objects for the transaction.
tx.setGasPayment([coin1, coin2]);
```

### Dapp / Wallet Integration

The Wallet Standard interface has been updated to support the `Transaction` kind directly. All `signTransaction` and `signAndExecuteTransaction` calls from dapps into wallets will be expected to provide a `Transaction` class. This transaction class can then be serialized and sent to your wallet for execution.

To serialize a transaction for sending to a wallet, we recommend using the `tx.serialize()` function, which returns an opaque string representation of the transaction that can be passed from the wallet standard dapp context to your wallet. This can then be converted back into a `Transaction` using `Transaction.from()`.

**Important:** The transaction should not be built from bytes in the dApp code. Using `serialize` instead of `build` allows you to build the transaction bytes within the wallet itself. This allows the wallet to perform gas logic and coin selection as needed.

```tsx
// Within a dApp
const tx = new Transaction();
wallet.signTransaction({ transaction: tx });

// Your wallet standard code:
function handleSignTransaction(input) {
  sendToWalletContext({ transaction: input.transaction.serialize() });
}

// Within your wallet context:
function handleSignRequest(input) {
  const userTx = Transaction.from(input.transaction);
}
```

## Sponsored Transactions

The transaction builder can support sponsored transaction by using the `onlyTransactionKind` flag when building the transaction.

```tsx
const tx = new Transaction();

// ... add some commands...

const kindBytes = await tx.build({ provider, onlyTransactionKind: true });

// Construct a sponsored transaction from the kind bytes:
const sponsoredTx = Transaction.fromKind(kindBytes);

// You can now set the sponsored transaction data that is required:
sponsoredTx.setSender(sender);
sponsoredTx.setGasOwner(sponsor);
sponsoredTx.setGasPayment(sponsorCoins);
```
