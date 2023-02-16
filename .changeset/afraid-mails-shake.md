---
"@mysten/sui.js": minor
---

Transaction signatures are now serialized into a single string, and all APIs that previously took the public key, signature, and scheme now just take the single serialized signature string. To help make parsing this easier, there are new `toSerializedSignature` and `fromSerializedSignature` methods exposed as well.
