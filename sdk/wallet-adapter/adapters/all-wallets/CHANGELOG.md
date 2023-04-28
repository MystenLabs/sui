# @mysten/wallet-adapter-all-wallets

## 0.5.4

### Patch Changes

- b4f0bfc76: Fix type definitions for package exports.
- Updated dependencies [b4f0bfc76]
  - @mysten/wallet-adapter-wallet-standard@0.7.4
  - @mysten/wallet-adapter-unsafe-burner@0.8.4

## 0.5.3

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.8.3
- @mysten/wallet-adapter-wallet-standard@0.7.3

## 0.5.2

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.8.2
- @mysten/wallet-adapter-wallet-standard@0.7.2

## 0.5.1

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.8.1
- @mysten/wallet-adapter-wallet-standard@0.7.1

## 0.5.0

### Minor Changes

- 19b567f21: Unified self- and delegated staking flows. Removed fields from `Validator` (`stake_amount`, `pending_stake`, and `pending_withdraw`) and renamed `delegation_staking_pool` to `staking_pool`. Additionally removed the `validator_stake` and `delegated_stake` fields in the `ValidatorSet` type and replaced them with a `total_stake` field.
- 5c3b00cde: Add object id to staking pool and pool id to staked sui.
- 3d9a04648: Adds `deactivation_epoch` to staking pool object, and adds `inactive_pools` to the validator set object.
- 0a7b42a6d: This changes almost all occurences of "delegate", "delegation" (and various capitalizations/forms) to their equivalent "stake"-based name. Function names, function argument names, RPC endpoints, Move functions, and object fields have been updated with this new naming convention.

### Patch Changes

- Updated dependencies [19b567f21]
- Updated dependencies [5c3b00cde]
- Updated dependencies [bf545c7d0]
- Updated dependencies [3d9a04648]
- Updated dependencies [da72e73a9]
- Updated dependencies [0a7b42a6d]
- Updated dependencies [c718deef4]
- Updated dependencies [68e60b02c]
- Updated dependencies [a6ffb8088]
- Updated dependencies [dbe73d5a4]
- Updated dependencies [64fb649eb]
  - @mysten/wallet-adapter-wallet-standard@0.7.0
  - @mysten/wallet-adapter-unsafe-burner@0.8.0

## 0.4.3

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.7.1
- @mysten/wallet-adapter-wallet-standard@0.6.3

## 0.4.2

### Patch Changes

- 0e202a543: Remove pending delegation switches.
- Updated dependencies [0e202a543]
- Updated dependencies [aa650aa3b]
  - @mysten/wallet-adapter-wallet-standard@0.6.2
  - @mysten/wallet-adapter-unsafe-burner@0.7.0

## 0.4.1

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.6.1
- @mysten/wallet-adapter-wallet-standard@0.6.1

## 0.4.0

### Minor Changes

- 473005d8f: Add protocol_version to CheckpointSummary and SuiSystemObject. Consolidate end-of-epoch information in CheckpointSummary.

### Patch Changes

- Updated dependencies [473005d8f]
  - @mysten/wallet-adapter-wallet-standard@0.6.0
  - @mysten/wallet-adapter-unsafe-burner@0.6.0

## 0.3.13

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.5.1
- @mysten/wallet-adapter-wallet-standard@0.5.1

## 0.3.12

### Patch Changes

- a8746d4e9: update SuiExecuteTransactionResponse
- Updated dependencies [96e883fc1]
- Updated dependencies [a8746d4e9]
  - @mysten/wallet-adapter-wallet-standard@0.5.0
  - @mysten/wallet-adapter-unsafe-burner@0.5.0

## 0.3.11

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.4.2
- @mysten/wallet-adapter-wallet-standard@0.4.2

## 0.3.10

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.4.1
- @mysten/wallet-adapter-wallet-standard@0.4.1

## 0.3.9

### Patch Changes

- Updated dependencies [65fd8ac17]
  - @mysten/wallet-adapter-wallet-standard@0.4.0
  - @mysten/wallet-adapter-unsafe-burner@0.4.0

## 0.3.8

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.3.4
- @mysten/wallet-adapter-wallet-standard@0.3.8

## 0.3.7

### Patch Changes

- Updated dependencies [c8bab06b0]
  - @mysten/wallet-adapter-unsafe-burner@0.3.3
  - @mysten/wallet-adapter-wallet-standard@0.3.7

## 0.3.6

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.3.2
- @mysten/wallet-adapter-wallet-standard@0.3.6

## 0.3.5

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.3.1
- @mysten/wallet-adapter-wallet-standard@0.3.5

## 0.3.4

### Patch Changes

- Updated dependencies [e6282ae71]
  - @mysten/wallet-adapter-unsafe-burner@0.3.0
  - @mysten/wallet-adapter-wallet-standard@0.3.4

## 0.3.3

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.2.3
- @mysten/wallet-adapter-wallet-standard@0.3.3

## 0.3.2

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.2.2
- @mysten/wallet-adapter-wallet-standard@0.3.2

## 0.3.1

### Patch Changes

- @mysten/wallet-adapter-unsafe-burner@0.2.1
- @mysten/wallet-adapter-wallet-standard@0.3.1

## 0.3.0

### Minor Changes

- 3ead1eefb: Remove legacy Sui Wallet adapter
- f5679be35: Introduce unsafe burner wallet adapter

### Patch Changes

- Updated dependencies [56de8448f]
- Updated dependencies [f5679be35]
  - @mysten/wallet-adapter-wallet-standard@0.3.0
  - @mysten/wallet-adapter-unsafe-burner@0.2.0

## 0.2.2

### Patch Changes

- Updated dependencies [06ba46f7d]
  - @mysten/wallet-adapter-mock-wallet@0.3.0
  - @mysten/wallet-adapter-sui-wallet@0.3.0
  - @mysten/wallet-adapter-wallet-standard@0.2.2

## 0.2.1

### Patch Changes

- @mysten/wallet-adapter-mock-wallet@0.2.1
- @mysten/wallet-adapter-sui-wallet@0.2.1
- @mysten/wallet-adapter-wallet-standard@0.2.1

## 0.2.0

### Minor Changes

- 5ac98bc9a: Introduce new wallet adapter based on the Wallet Standard. This wallet adapter automatically detects wallets that adhere to the standard interface.

### Patch Changes

- 5ac98bc9a: Add support for standard wallet adapter.
- Updated dependencies [5ac98bc9a]
- Updated dependencies [5ac98bc9a]
  - @mysten/wallet-adapter-sui-wallet@0.2.0
  - @mysten/wallet-adapter-mock-wallet@0.2.0
  - @mysten/wallet-adapter-wallet-standard@0.2.0

## 0.1.0

### Minor Changes

- d343b67e: Re-release packages

### Patch Changes

- Updated dependencies [d343b67e]
  - @mysten/wallet-adapter-mock-wallet@0.1.0
  - @mysten/wallet-adapter-sui-wallet@0.1.0

## 0.0.1

### Patch Changes

- e1d39d62: Update build process for wallet adapters to expose ES modules as well as CommonJS builds.
- e2aa08e9: Fix missing built files for packages.
- b9ee5c22: Add changesets to manage release for wallet adapters.
- Updated dependencies [e1d39d62]
- Updated dependencies [e2aa08e9]
- Updated dependencies [b9ee5c22]
  - @mysten/wallet-adapter-mock-wallet@0.0.1
  - @mysten/wallet-adapter-sui-wallet@0.0.1
