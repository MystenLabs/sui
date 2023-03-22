---
"@mysten/wallet-adapter-wallet-standard": minor
"@mysten/wallet-adapter-unsafe-burner": minor
"@mysten/wallet-adapter-base": minor
"@mysten/wallet-adapter-all-wallets": minor
"@mysten/wallet-kit-core": minor
"@mysten/wallet-standard": minor
"@mysten/wallet-kit": minor
"@mysten/sui.js": minor
"@mysten/bcs": minor
---

Unified self- and delegated staking flows. Removed fields from `Validator` (`stake_amount`, `pending_stake`, and `pending_withdraw`) and renamed `delegation_staking_pool` to `staking_pool`. Additionally removed the `validator_stake` and `delegated_stake` fields in the `ValidatorSet` type and replaced them with a `total_stake` field.
