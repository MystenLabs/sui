---
"@mysten/sui.js": patch
---

selectCoinsWithBalanceGreaterThanOrEqual and selectCoinWithBalanceGreaterThanOrEqual uses CoinStruct instead of ObjectDataFull. Coin.totalBalance, sortByBalance expects CoinStruct. Added getBalanceFromCoinStruct.
