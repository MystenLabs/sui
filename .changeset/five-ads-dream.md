---
'@mysten/sui': minor
---

Add new tx.object methods for defining inputs for well known object ids:

- `tx.object.system()`: `0x5`
- `tx.object.clock()`: `0x6`
- `tx.object.random()`: `0x8`
- `tx.object.denyList()`: `0x403`
