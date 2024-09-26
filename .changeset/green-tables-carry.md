---
'@mysten/sui': minor
'@mysten/bcs': minor
---

Updated hex, base64, and base58 utility names for better consistency

All existing methods will continue to work, but the following methods have been deprecated and replaced with methods with improved names:

- `toHEX` -> `toHEX`
- `fromHEX` -> `fromHex`
- `toB64` -> `toBase64`
- `fromB64` -> `fromBase64`
- `toB58` -> `toBase58`
- `fromB58` -> `fromBase58`
