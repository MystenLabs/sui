# Change Log

## 0.6.1

### Patch Changes

- 0e202a543: Remove pending delegation switches.

## 0.6.0

### Minor Changes

- 598f106ef: ### Adds base58 encoding support to bcs

  - two functions added: `fromB58` and `toB58` similar to existing encodings
  - `Reader.toString` and `de/encodeStr` methods support new `base58` value
  - adds a 3 built-in types "hex-string", "base58-string" and "base64-string"
  - adds constants for the built-ins: `BCS.BASE64`, `BCS.BASE58` and `BCS.HEX`

  ```js
  bcs.registerStructType("TestStruct", {
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
  let struct = { name: "Alice", age: 25 };
  let bytes = bcs.ser({ name: "string", age: "u8" }, struct).toBytes();
  let restored = bcs.de({ name: "string", age: "u8" }, bytes);

  // `restored` deeply equals `struct`
  ```

  ```js
  // aliases for types
  bcs.registerAlias("Name", "string");
  bcs.ser("Name", "Palpatine");
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
