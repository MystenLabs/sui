# Kiosk SDK

> **This package is still in active development. Use at your own risk**.
> Currently, the only supported environment is **Sui Testnet**.

This Kiosk SDK library provides different utilities to interact/create/manage a [Kiosk](https://github.com/MystenLabs/sui/tree/main/kiosk).

## Installation

To install, add `@mysten/kiosk` package to your project

```
npm i @mysten/kiosk
```

You can also use your preferred package manager, such as yarn or pnpm.

## Examples

Here are some indicative examples on how to use the kiosk SDK.

<details>
<summary>Getting the listings & items by the kiosk's id</summary>

```typescript
import { fetchKiosk } from '@mysten/kiosk';
import { Connection, JsonRpcProvider } from '@mysten/sui.js';

const provider = new JsonRpcProvider(
  new Connection({ fullnode: 'https://fullnode.testnet.sui.io:443' }),
);

const getKiosk = async () => {
  const kioskAddress = `0xSomeKioskAddress`;

  const {
    data: res,
    nextCursor,
    hasNextPage,
  } = await fetchKiosk(
    provider,
    kioskAddress,
    { limit: 100 },
    { withListingPrices: true, withKioskFields: true },
  ); // could also add `cursor` for pagination

  console.log(res); // { items: [],  itemIds: [],  listingIds: [], kiosk: {...} }
  console.log(nextCursor); // null
  console.log(hasNextPage); // false
};
```

</details>

<details>
<summary>Purchasing an item (currently supports royalty rule deployed on testnet or no rules)</summary>

```typescript
import { fetchKiosk } from '@mysten/kiosk';
import { Connection, JsonRpcProvider } from '@mysten/sui.js';

const provider = new JsonRpcProvider(
  new Connection({ fullnode: 'https://fullnode.testnet.sui.io:443' }),
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
const purchaseItem = async (item, kioskId) => {

  // fetch the policy of the item (could be an array, if there's more than one transfer policy)
  const policies = await queryTransferPolicy(provider, item.type);
  // selecting the first one for simplicity.
  const policyId = policy[0]?.id;
  // initialize tx block.
  const tx = new TransactionBlock();

  // Purchases the item. Right now it also resolves a royalty rule, if one exists.
  // There will be some additional work to support further rules & custom ones soon.
  const purchasedItem = purchaseAndResolvePolicies(tx, item.type, item.listing, kioskId, item.objectId, policy[0]);

  // now we need to decide what to do with the item
  // ... e.g. place() // places the item to the user's kiosk.
  // (NOT YET SUPPORTED BUT WORTH MENTIONING if the item has the `kiosk_lock` rule, the resolver will place it in the kiosk automatically.
  // For now, to support this rule, we need to manually resolve the `kiosk_lock` rule and place it in our owned kiosk.)

  // ...finally, sign PTB & execute it.

};
```

</details>

<details>
<summary>Create a kiosk, share it and get transfer the `kioskOwnerCap` to the wallet's address</summary>

```typescript
import { createKioskAndShare } from '@mysten/kiosk';
import { TransactionBlock } from '@mysten/sui.js';

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
import { TransactionBlock } from '@mysten/sui.js';

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
import { TransactionBlock } from '@mysten/sui.js';

const withdraw = async () => {
  const kiosk = 'SomeKioskId';
  const kioskCap = 'KioskCapObjectId';
  const amount = '100000';

  const tx = new TransactionBlock();

  withdrawFromKiosk(tx, kiosk, kioskCap, amount);

  // ... continue to sign and execute the transaction
  // ...
};
```

</details>
