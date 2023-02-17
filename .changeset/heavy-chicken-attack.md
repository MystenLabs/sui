---
"@mysten/bcs": minor
---

Adds base58 encoding support to bcs

- two functions added: `fromB58` and `toB58` similar to existing encodings
- `Reader.toString` and `de/encodeStr` methods support new `base58` value
- adds a 3 built-in types "hex-string", "base58-string" and "base64-string"
- adds constants for the built-ins: `BCS.BASE64`, `BCS.BASE58` and `BCS.HEX`

To access new built-in types, you use them directly in the struct definition:

```js
bcs.registerStructType("TestStruct", {
  hex: BCS.HEX,
  base58: BCS.BASE58,
  base64: BCS.BASE64,
});
```
