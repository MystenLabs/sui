---
'@mysten/wallet-adapter-unsafe-burner': minor
'@mysten/wallet-kit': minor
'@mysten/suins-toolkit': minor
'@mysten/deepbook': minor
'@mysten/kiosk': minor
---

Update to use modular imports from @mysten/sui.js

Some methods now accept a `SuiClient` imported from `@mysten/sui.js/client` rather than a `JsonRpcProvider`
