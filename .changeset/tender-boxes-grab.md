---
"@mysten/sui.js": minor
---

Removed usage of `cross-fetch` in the TypeScript SDK. If you are running in an environment that does not have `fetch` defined, you will need to polyfill it.
