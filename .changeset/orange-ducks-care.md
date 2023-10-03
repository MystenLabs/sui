---
'@mysten/sui.js': minor
---

Improve APIs for building transaction inputs

- txb.splitCoins now accepts `amounts`` as raw JavaScript number
- txb.transferObjects now accepts `address` as JavaScript string
- All single objects, or lists of objects, now also accepts object IDs as JavaScript strings
- txb.pure accepts `SerializedBcs` (eg `txb.pure(bcs.U64.serialize(123))`)
- Added pure helpers (`txb.pure.address()`, `txb.bool()`, and `txb.pure.u{8-256}()`) to simplify serialization of pure values
- Deprecated using `txb.pure` with raw JavaScript values, or an explicit type argument.
