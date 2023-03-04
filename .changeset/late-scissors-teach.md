---
"@mysten/sui.js": minor
---

Consolidate get_object and get_raw_object into a single get_object endpoint which now takes an additional config parameter with type `SuiObjectDataOptions` and has a new return type `SuiObjectResponse`. By default, only object_id, version, and digest are fetched.
