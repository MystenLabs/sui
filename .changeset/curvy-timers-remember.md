---
"@mysten/sui.js": minor
---

Flip the default value of `skipDataValidation` to true in order to mitigate the impact of breaking changes on applications. When there's a mismatch between the Typescript definitions and RPC response, the SDK now log a console warning instead of throwing an error.
