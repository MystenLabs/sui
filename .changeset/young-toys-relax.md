---
'@mysten/kiosk': patch
---

Fixes `lock` function arguments. `itemId` is replaced by `item`, which accepts an ObjectArgument instead of a string. `itemId` is still supported but deprecated, and will be removed in future versions.
