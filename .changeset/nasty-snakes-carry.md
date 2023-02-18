---
"@mysten/bcs": minor
---

### Adds base58 encoding support to bcs

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

###  Examples

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
