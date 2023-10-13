# Change Log

## 0.8.1

### Patch Changes

- b48289346: Mark packages as being side-effect free.

## 0.8.0

### Minor Changes

- 1bc430161: Add new type-safe schema builder. See https://sui-typescript-docs.vercel.app/bcs for updated documentation
- e4484852b: Add isSerializedBcs helper

## 0.7.4

### Patch Changes

- 290c8e640: Fix parsing of hex strings where leading 0s have been trimmed

## 0.7.3

### Patch Changes

- 36f2edff3: Fix an issue with parsing struct types with nested type parameters

## 0.7.2

### Patch Changes

- ca5c72815d: Fix a bcs decoding bug for u128 and u256 values
- fdb569464e: Fixes an issue with a top level generic in a nested vector

## 0.7.1

### Patch Changes

- b4f0bfc76: Fix type definitions for package exports.

## 0.7.0

### Minor Changes

- 19b567f21: Unified self- and delegated staking flows. Removed fields from `Validator` (`stake_amount`, `pending_stake`, and `pending_withdraw`) and renamed `delegation_staking_pool` to `staking_pool`. Additionally removed the `validator_stake` and `delegated_stake` fields in the `ValidatorSet` type and replaced them with a `total_stake` field.
- 5c3b00cde: Add object id to staking pool and pool id to staked sui.
- 3d9a04648: Adds `deactivation_epoch` to staking pool object, and adds `inactive_pools` to the validator set object.
- a8049d159: Fixes the issue with deep nested generics by introducing array type names

  - all of the methods (except for aliasing) now allow passing in arrays instead
    of strings to allow for easier composition of generics and avoid using template
    strings

  ```js
  // new syntax
  bcs.registerStructType(['VecMap', 'K', 'V'], {
  	keys: ['vector', 'K'],
  	values: ['vector', 'V'],
  });

  // is identical to an old string definition
  bcs.registerStructType('VecMap<K, V>', {
  	keys: 'vector<K>',
  	values: 'vector<V>',
  });
  ```

  Similar approach applies to `bcs.ser()` and `bcs.de()` as well as to other register\* methods

- a0955c479: Switch from 20 to 32-byte address. Match Secp256k1.deriveKeypair with Ed25519.
- 0a7b42a6d: This changes almost all occurences of "delegate", "delegation" (and various capitalizations/forms) to their equivalent "stake"-based name. Function names, function argument names, RPC endpoints, Move functions, and object fields have been updated with this new naming convention.
- 77bdf907f: When parsing u64, u128, and u256 values with bcs, they are now string encoded.

## 0.6.1

### Patch Changes

- 0e202a543: Remove pending delegation switches.

## 0.6.0

```js
// new syntax
bcs.registerStructType(['VecMap', 'K', 'V'], {
	keys: ['vector', 'K'],
	values: ['vector', 'V'],
});

// is identical to an old string definition
bcs.registerStructType('VecMap<K, V>', {
	keys: 'vector<K>',
	values: 'vector<V>',
});
```

### Minor Changes

- 598f106ef: Adds base58 encoding support to bcs

- two functions added: `fromB58` and `toB58` similar to existing encodings
- `Reader.toString` and `de/encodeStr` methods support new `base58` value
- adds a 3 built-in types "hex-string", "base58-string" and "base64-string"
- adds constants for the built-ins: `BCS.BASE64`, `BCS.BASE58` and `BCS.HEX`

```js
bcs.registerStructType('TestStruct', {
	hex: BCS.HEX,
	base58: BCS.BASE58,
	base64: BCS.BASE64,
});
```

### Adds type aliasing and inline definitions

- adds new `registerAlias` function which allows type aliases and tracks basic recursion
- adds support for inline definitions in the `.de()` and `.ser()` methods

### Examples

```js
// inline definition example
let struct = { name: 'Alice', age: 25 };
let bytes = bcs.ser({ name: 'string', age: 'u8' }, struct).toBytes();
let restored = bcs.de({ name: 'string', age: 'u8' }, bytes);

// `restored` deeply equals `struct`
```

```js
// aliases for types
bcs.registerAlias('Name', 'string');
bcs.ser('Name', 'Palpatine');
```

## 0.5.0

### Minor Changes

- 1a0968636: Remove usage of bn.js, and use native bigints instead.

## 0.4.0

### Minor Changes

- 1591726e8: Support multiple instances of BCS

### Patch Changes

- 1591726e8: Add support for generic types

## 0.3.0

### Minor Changes

- d343b67e: Re-release packages

## 0.2.1

### Patch Changes

- c5e4851b: Updated build process from TSDX to tsup.
- e2aa08e9: Fix missing built files for packages.

Version history from v0.1.0 to this day.

## v0.2.0 - Usability Boost

- `bcs.de(...)` now supports strings if encoding is passed as the last argument
- `BCS` (upper) -> `bcs` (lower) renaming
- Improved documentation, checked documentation examples for failures

## v0.1.0

First version of libary published.
