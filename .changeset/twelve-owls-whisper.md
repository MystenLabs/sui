---
"@mysten/bcs": minor
---

Adds base58 encoding support to bcs

- two functions added: `fromB58` and `toB58` similar to existing encodings
- `Reader.toString` and `de/encodeStr` methods support new `base58` value
