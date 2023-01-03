---
"@mysten/sui.js": minor
---

Rebuilt type-narrowing utilties (e.g. `isSuiObject`) on top of Superstruct, which should make them more reliable.
The type-narrowing functions are no longer exported, instead a Superstruct schema is exported, in addition to an `is` and `assert` function, both of which can be used to replace the previous narrowing functions. For example, `isSuiObject(data)` becomes `is(data, SuiObject)`.
