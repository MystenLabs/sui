# Kiosk SDK

> **This package is still in active development. Use at your own risk**.

This Kiosk SDK library provides different utilities to interact/create/manage a
[Kiosk](https://github.com/MystenLabs/sui/tree/main/kiosk).

## Installation

To install, add `@mysten/kiosk` package to your project

```
npm i @mysten/kiosk
```

You can also use your preferred package manager, such as yarn or pnpm.

## Examples

Here are some indicative examples on how to use the kiosk SDK.

<details>
<summary>Find the kiosks owned by an address</summary>

```typescript
import { getOwnedKiosks } from '@mysten/kiosk';
import { SuiClient } from '@mysten/sui.js/client';

const client = new SuiClient(
  url: 'https://fullnode.testnet.sui.io:443',
);


// You could use these to fetch the contents for each kiosk, or use the `kioskOwnerCap` data for other actions.
const getUserKiosks = async () => {
	const address = `0xAddress`;
	const { data } = await getOwnedKiosks(client, address);
	console.log(data); // kioskOwnerCaps:[], kioskIds: []
};
```

</details>

<details>
<summary>Getting the listings & items by the kiosk's ID</summary>

```typescript
import { fetchKiosk } from '@mysten/kiosk';
import { SuiClient } from '@mysten/sui.js/client';

const client = new SuiClient(
	url: 'https://fullnode.testnet.sui.io:443',
);

const getKiosk = async () => {
	const kioskAddress = `0xSomeKioskAddress`;

	const { data } = await fetchKiosk(
		client,
		kioskAddress,
		{}, // empty pagination, currently disabled.
		{ withListingPrices: true, withKioskFields: true },
	);

	console.log(data); // { items: [],  itemIds: [],  listingIds: [], kiosk: {...} }
};
```

</details>

<details>
<summary>Purchasing an item (currently supports royalty rule, kiosk_lock_rule, no rules, (combination works too))</summary>

```typescript
import { queryTransferPolicy, purchaseAndResolvePolicies, place, testnetEnvironment } from '@mysten/kiosk';
import { SuiClient } from '@mysten/sui.js/client';

const client = new SuiClient(
  url: 'https://fullnode.testnet.sui.io:443',
);

 // the kiosk we're purchasing from
const kioskId = `0xSomeKioskAddress`;
// a sample item retrieved from `fetchKiosk` function (or hard-coded)
const item = {
  isLocked: false,
  objectId: "0xb892d61a9992a10c9453efcdbd14ca9720d7dc1000a2048224209c9e544ed223"
  type: "0x52852c4ba80040395b259c641e70b702426a58990ff73cecf5afd31954429090::test::TestItem",
  listing: {
    isExclusive: false,
    listingId: "0x368b512ff2514dbea814f26ec9a3d41198c00e8ed778099961e9ed22a9f0032b",
    price: "20000000000" // in MIST
  }
}
const ownedKiosk = `0xMyKioskAddress`;
const ownedKioskCap = `0xMyKioskOwnerCap`;

const purchaseItem = async (item, kioskId) => {

  // fetch the policy of the item (could be an array, if there's more than one transfer policy)
  const policies = await queryTransferPolicy(client, item.type);
  // selecting the first one for simplicity.
  const policyId = policy[0]?.id;
  // initialize tx block.
  const tx = new TransactionBlock();

  // Both are required if you there is a `kiosk_lock_rule`.
  // Optional otherwise. Function will throw an error if there's a kiosk_lock_rule and these are missing.
  const extraParams = {
    ownedKiosk,
    ownedKioskCap
  }
  // Define the environment.
  // To use a custom package address for rules, you could call:
  // const environment = customEnvironment('<PackageAddress>');
  const environment = testnetEnvironment;

  // Extra params. Optional, but required if the user tries to resolve a `kiosk_lock_rule`.
  // Purchases the item. Supports `kiosk_lock_rule`, `royalty_rule` (accepts combination too).
  const result = purchaseAndResolvePolicies(tx, item.type, item.listing.price, kioskId, item.objectId, policy[0], environment, extraParams);

  // result = {item: <the_purchased_item>, canTransfer: true/false // depending on whether there was a kiosk lock rule }
  // if the item didn't have a kiosk_lock_rule, we need to do something with it.
  // for e..g place it in our own kiosk. (demonstrated below)
  if(result.canTransfer) place(tx, item.type, ownedKiosk, ownedKioskCap , result.item);

  // ...finally, sign PTB & execute it.

};
```

</details>

<details>
<summary>Create a kiosk, share it and get transfer the `kioskOwnerCap` to the wallet's address</summary>

```typescript
import { createKioskAndShare } from '@mysten/kiosk';
import { TransactionBlock } from '@mysten/sui.js/transactions';

const createKiosk = async () => {
	const accountAddress = '0xSomeSuiAddress';

	const tx = new TransactionBlock();
	const kiosk_cap = createKioskAndShare(tx);

	tx.transferObjects([kiosk_cap], tx.pure(accountAddress, 'address'));

	// ... continue to sign and execute the transaction
	// ...
};
```

</details>

<details>
<summary>Place an item and list it for sale in the kiosk</summary>

```typescript
import { placeAndList } from '@mysten/kiosk';
import { TransactionBlock } from '@mysten/sui.js/transactions';

const placeAndListToKiosk = async () => {
	const kiosk = 'SomeKioskId';
	const kioskCap = 'KioskCapObjectId';
	const itemType = '0xItemAddr::some:ItemType';
	const item = 'SomeItemId';
	const price = '100000';

	const tx = new TransactionBlock();

	placeAndList(tx, itemType, kiosk, kioskCap, item, price);

	// ... continue to sign and execute the transaction
	// ...
};
```

</details>

<details>
<summary>Withdraw profits from your kiosk</summary>

```typescript
import { withdrawFromKiosk } from '@mysten/kiosk';
import { TransactionBlock } from '@mysten/sui.js/transactions';

const withdraw = async () => {
	const kiosk = 'SomeKioskId';
	const kioskCap = 'KioskCapObjectId';
	const address = '0xSomeAddressThatReceivesTheFunds';
	const amount = '100000';

	const tx = new TransactionBlock();

	withdrawFromKiosk(tx, kiosk, kioskCap, amount);

	// transfer the Coin to self or any other address.
	tx.transferObjects([coin], tx.pure(address, 'address'));

	// ... continue to sign and execute the transaction
	// ...
};
```

</details>

<details>

<summary>Create a Transfer Policy for a Type</summary>

You can create a TransferPolicy only for packages for which you own the `publisher` object.

As a best practice, you should use a single Transfer policy per type (T). If you use more than one
policy for a type, and the rules for each differ, anyone that meets the conditions for any of the
attached policies can purchase an asset from a kiosk. You can't specify which policy applies to a
specific asset for the type when there is more than one policy attached. When someone meets the
conditions that are easiest to meet, they are allowed to purchase and transfer the asset.

Before you create a transfer policy, you can use the `queryTransferpolicy` function to check the
transfer policy associated with a type. This is similar to the `purchaseAndResolvePolicies` example
above.

```typescript
import { createTransferPolicy } from '@mysten/kiosk';
import { TransactionBlock } from '@mysten/sui.js';

const createPolicyForType = async () => {
	const type = 'SomePackageId::type::MyType'; // the Type for which we're creating a Transfer Policy.
	const publisher = 'publisherObjectId'; // the publisher object id that you got when claiming the package that defines the Type.
	const address = 'AddressToReceiveTheCap';

	const tx = new TransactionBlock();
	// create transfer policy
	let transferPolicyCap = createTransferPolicy(tx, type, publisher);
	// transfer the Cap to the address.
	tx.transferObjects([transferPolicyCap], tx.pure(address, 'address'));

	// ... continue to sign and execute the transaction
	// ...
};
```

</details>

<details>

<summary>Attach Royalty and Lock rules to a Transfer policy</summary>

```typescript
import {
	createTransferPolicy,
	attachKioskLockRule,
	attachRoyaltyRule,
	testnetEnvironment,
	percentageToBasisPoints,
} from '@mysten/kiosk';
import { TransactionBlock } from '@mysten/sui.js';

// Attaches a royalty rule of 1% or 0.1 SUI (whichever is bigger)
// as well as a kiosk lock, making the objects trade-able only from/to a kiosk.
const attachStrongRoyalties = async () => {
	const type = 'SomePackageId::type::MyType'; // the Type for which we're attaching rules.
	const policyId = 'policyObjectId'; // the transfer Policy ID that was created for that Type.
	const transferPolicyCap = 'transferPolicyCapId'; // the transferPolicyCap for that policy.

	// royalties configuration.
	const percentage = 2.55; // 2.55%
	const minAmount = 100_000_000; // 0.1 SUI.

	// the environment on which we're referecing the rules package.
	// use `mainnetEnvironment` for mainnet.
	const enviroment = testnetEnvironment;

	const tx = new TransactionBlock();

	attachKioskLockRule(tx, type, policyId, policyCapId, enviroment);
	attachRoyaltyRule(
		tx,
		type,
		policyId,
		policyCapId,
		percentageToBasisPoints(percentage),
		minAmount,
		enviroment,
	);

	// ... continue to sign and execute the transaction
	// ...
};
```

</details>
