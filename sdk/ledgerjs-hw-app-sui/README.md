[Ledger Github](https://github.com/LedgerHQ/ledgerjs/),
[Ledger Developer Portal](https://developers.ledger.com/),
[Ledger Developer Discord](https://developers.ledger.com/discord-pro)

# ledgerjs-hw-app-sui

[Ledger Hardware Wallet](https://www.ledger.com/) JavaScript bindings for [Sui](https://sui.io/),
based on [LedgerJS](https://github.com/LedgerHQ/ledgerjs).

## Using LedgerJS for Sui

Here is a sample app for Node:

```javascript
const Transport = require('@ledgerhq/hw-transport').default;
const Sui = require('@mysten/ledgerjs-hw-app-sui').default;

const getPublicKey = async () => {
	const sui = new Sui(await Transport.create());
	return await sui.getPublicKey("44'/784'/0'/0'/0'");
};

const signTransaction = async () => {
	const sui = new Sui(await Transport.create());
	return await sui.signTransaction("44'/784'/0'/0'/0'", '<transaction contents>');
};

const getVersion = async () => {
	const sui = new Sui(await Transport.create());
	return await sui.getVersion();
};

const doAll = async () => {
	console.log(await getPublicKey());
	console.log(await signTransaction());
	console.log(await getVersion());
};

doAll().catch((err) => console.log(err));
```

## API

### Table of Contents

- [Sui](#sui)
  - [Parameters](#parameters)
  - [Examples](#examples)
  - [getPublicKey](#getpublickey)
    - [Parameters](#parameters-1)
    - [Examples](#examples-1)
  - [signTransaction](#signtransaction)
    - [Parameters](#parameters-2)
    - [Examples](#examples-2)
  - [getVersion](#signtransaction)
    - [Parameters](#parameters-3)
    - [Examples](#examples-3)

### Parameters

- `transport` **`Transport<any>`**
- `scrambleKey`
  **[string](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/String)**
  (optional, default `"Sui"`)

### Examples

```javascript
import Sui from 'ledgerjs-hw-app-sui';

const transport = await Transport.create();
const sui = new Sui(transport);
```

### getPublicKey

Gets the Sui address for a given BIP-32 path.

#### Parameters

- `path`
  **[string](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/String)** a
  path in BIP-32 format
- `displayOnDevice`
  **[boolean](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Boolean)**
  whether or not to display the address on the Ledger device.

#### Examples

```javascript
const publicKey = await sui.getPublicKey("44'/784'/0'/0'/0'");
```

Returns
**[Promise](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Promise)&lt;[string](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/String)>**
an object with a public key.

### signTransaction

Sign a transaction with a given BIP-32 path.

#### Parameters

- `path`
  **[string](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/String)** a
  path in BIP-32 format

#### Examples

```javascript
const publicKey = await sui.signTransaction("44'/784'/0'/0'/0'", '<transaction contents>');
```

Returns
**[Promise](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Promise)&lt;[object](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Object)>**
an object with text field containing a signature.

### getVersion

Get the version of the application installed on the hardware device.

#### Examples

```javascript
console.log(await sui.getVersion());
```

for version 0.1.0, it produces something like

```
{
  major: 0
  minor: 1
  patch: 0
}
```

Returns
**[Promise](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Promise)&lt;{[object](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Object)}>**
an object with major, minor, and patch of the version.
