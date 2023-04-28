# @mysten/wallet-adapter-unsafe-burner

## 0.8.4

### Patch Changes

- b4f0bfc76: Fix type definitions for package exports.
- Updated dependencies [4ae3cbea3]
- Updated dependencies [d2755a496]
- Updated dependencies [f612dac98]
- Updated dependencies [c219e7470]
- Updated dependencies [59ae0e7d6]
- Updated dependencies [c219e7470]
- Updated dependencies [4e463c691]
- Updated dependencies [b4f0bfc76]
  - @mysten/sui.js@0.32.2
  - @mysten/wallet-adapter-base@0.7.4
  - @mysten/wallet-standard@0.5.4

## 0.8.3

### Patch Changes

- Updated dependencies [3224ffcd0]
  - @mysten/sui.js@0.32.1
  - @mysten/wallet-adapter-base@0.7.3
  - @mysten/wallet-standard@0.5.3

## 0.8.2

### Patch Changes

- Updated dependencies [9b42d0ada]
  - @mysten/sui.js@0.32.0
  - @mysten/wallet-adapter-base@0.7.2
  - @mysten/wallet-standard@0.5.2

## 0.8.1

### Patch Changes

- Updated dependencies [976d3e1fe]
- Updated dependencies [0419b7c53]
- Updated dependencies [f3c096e3a]
- Updated dependencies [5a4e3e416]
- Updated dependencies [27dec39eb]
  - @mysten/sui.js@0.31.0
  - @mysten/wallet-adapter-base@0.7.1
  - @mysten/wallet-standard@0.5.1

## 0.8.0

### Minor Changes

- 19b567f21: Unified self- and delegated staking flows. Removed fields from `Validator` (`stake_amount`, `pending_stake`, and `pending_withdraw`) and renamed `delegation_staking_pool` to `staking_pool`. Additionally removed the `validator_stake` and `delegated_stake` fields in the `ValidatorSet` type and replaced them with a `total_stake` field.
- 5c3b00cde: Add object id to staking pool and pool id to staked sui.
- 3d9a04648: Adds `deactivation_epoch` to staking pool object, and adds `inactive_pools` to the validator set object.
- da72e73a9: Change the address of Move package for staking and validator related Move modules.
- 0a7b42a6d: This changes almost all occurences of "delegate", "delegation" (and various capitalizations/forms) to their equivalent "stake"-based name. Function names, function argument names, RPC endpoints, Move functions, and object fields have been updated with this new naming convention.
- c718deef4: wallet-standard: changes sui:signAndExecuteTransaction and sui:signTransaction features to support account and chain options
  wallet-adapter-wallet-standard: change signAndExecuteTransaction and signTransaction signatures to support account and chain options
  wallet-adapter-wallet-standard: ensure version compatibility for of the wallet signAndExecuteTransaction and signTransaction features before using them (same major version)
  wallet-kit-core/wallet-kit: expose accounts as ReadonlyWalletAccount instead of only the address
  wallet-kit-core: signTransaction and signAndExecuteTransaction methods mirror the ones in standard adapter
- 68e60b02c: Changed where the options and requestType for signAndExecuteTransaction are.
- a6ffb8088: Removed events from transaction effects, TransactionEvents will now be provided in the TransactionResponse, along side TransactionEffects.
- dbe73d5a4: Add an optional `contentOptions` field to `SuiSignAndExecuteTransactionOptions` to specify which fields to include in `SuiTransactionBlockResponse` (e.g., transaction, effects, events, etc). By default, only the transaction digest will be included.
- 64fb649eb: Remove old `SuiExecuteTransactionResponse` interface, and `CertifiedTransaction` interface in favor of the new unified `SuiTransactionBlockResponse` interfaces.

### Patch Changes

- Updated dependencies [956ec28eb]
- Updated dependencies [4adfbff73]
- Updated dependencies [4c4573ebe]
- Updated dependencies [acc2edb31]
- Updated dependencies [941b03af1]
- Updated dependencies [a6690ac7d]
- Updated dependencies [a211dc03a]
- Updated dependencies [4c1e331b8]
- Updated dependencies [19b567f21]
- Updated dependencies [7659e2e91]
- Updated dependencies [0d3cb44d9]
- Updated dependencies [00bb9bb66]
- Updated dependencies [36c264ebb]
- Updated dependencies [891abf5ed]
- Updated dependencies [2e0ef59fa]
- Updated dependencies [33cb357e1]
- Updated dependencies [6bd88570c]
- Updated dependencies [f1e42f792]
- Updated dependencies [272389c20]
- Updated dependencies [3de8de361]
- Updated dependencies [be3c4f51e]
- Updated dependencies [dbe73d5a4]
- Updated dependencies [14ba89144]
- Updated dependencies [c82e4b454]
- Updated dependencies [7a2eaf4a3]
- Updated dependencies [2ef2bb59e]
- Updated dependencies [9b29bef37]
- Updated dependencies [8700809b5]
- Updated dependencies [5c3b00cde]
- Updated dependencies [01272ab7d]
- Updated dependencies [9822357d6]
- Updated dependencies [bf545c7d0]
- Updated dependencies [3d9a04648]
- Updated dependencies [da72e73a9]
- Updated dependencies [0672b5990]
- Updated dependencies [a0955c479]
- Updated dependencies [3eb3a1de8]
- Updated dependencies [0c9047698]
- Updated dependencies [4593333bd]
- Updated dependencies [d5ef1b6e5]
- Updated dependencies [0a7b42a6d]
- Updated dependencies [3de8de361]
- Updated dependencies [dd348cf03]
- Updated dependencies [c718deef4]
- Updated dependencies [57c17e02a]
- Updated dependencies [65f1372dd]
- Updated dependencies [a09239308]
- Updated dependencies [fe335e6ba]
- Updated dependencies [5dc25faad]
- Updated dependencies [64234baaf]
- Updated dependencies [79c2165cb]
- Updated dependencies [d3170ba41]
- Updated dependencies [68e60b02c]
- Updated dependencies [a6ffb8088]
- Updated dependencies [3304eb83b]
- Updated dependencies [4189171ef]
- Updated dependencies [210840114]
- Updated dependencies [77bdf907f]
- Updated dependencies [a74df16ec]
- Updated dependencies [0f7aa6507]
- Updated dependencies [9b60bf700]
- Updated dependencies [dbe73d5a4]
- Updated dependencies [64fb649eb]
- Updated dependencies [a6b0c4e5f]
  - @mysten/wallet-standard@0.5.0
  - @mysten/sui.js@0.30.0
  - @mysten/wallet-adapter-base@0.7.0

## 0.7.1

### Patch Changes

- Updated dependencies [31bfcae6a]
  - @mysten/sui.js@0.29.1
  - @mysten/wallet-adapter-base@0.6.3

## 0.7.0

### Minor Changes

- aa650aa3b: Introduce new `Connection` class, which is used to define the endpoints that are used when interacting with the network.

### Patch Changes

- 0e202a543: Remove pending delegation switches.
- Updated dependencies [f1e3a0373]
- Updated dependencies [f2e713bd0]
- Updated dependencies [0e202a543]
- Updated dependencies [67e503c7c]
- Updated dependencies [4baf554f1]
- Updated dependencies [aa650aa3b]
- Updated dependencies [6ff0c785f]
  - @mysten/sui.js@0.29.0
  - @mysten/wallet-adapter-base@0.6.2

## 0.6.1

### Patch Changes

- Updated dependencies [a67cc044b]
- Updated dependencies [24bdb66c6]
- Updated dependencies [a67cc044b]
- Updated dependencies [a67cc044b]
  - @mysten/sui.js@0.28.0
  - @mysten/wallet-adapter-base@0.6.1

## 0.6.0

### Minor Changes

- 473005d8f: Add protocol_version to CheckpointSummary and SuiSystemObject. Consolidate end-of-epoch information in CheckpointSummary.

### Patch Changes

- Updated dependencies [473005d8f]
- Updated dependencies [fcba70206]
- Updated dependencies [59641dc29]
- Updated dependencies [ebe6c3945]
- Updated dependencies [629804d26]
- Updated dependencies [f51c85e85]
- Updated dependencies [e630f6832]
  - @mysten/wallet-adapter-base@0.6.0
  - @mysten/sui.js@0.27.0

## 0.5.1

### Patch Changes

- Updated dependencies [97c46ca9d]
  - @mysten/sui.js@0.26.1
  - @mysten/wallet-adapter-base@0.5.1

## 0.5.0

### Minor Changes

- 96e883fc1: Update wallet adapter and wallet standard to support passing through the desired request type.

### Patch Changes

- a8746d4e9: update SuiExecuteTransactionResponse
- Updated dependencies [034158656]
- Updated dependencies [96e883fc1]
- Updated dependencies [a8746d4e9]
- Updated dependencies [57fc4dedd]
- Updated dependencies [e6a71882f]
- Updated dependencies [e6a71882f]
- Updated dependencies [21781ba52]
- Updated dependencies [b3ba6dfbc]
  - @mysten/sui.js@0.26.0
  - @mysten/wallet-adapter-base@0.5.0

## 0.4.2

### Patch Changes

- Updated dependencies [ebfdd5c56]
- Updated dependencies [7b4bf43bc]
- Updated dependencies [72481e759]
- Updated dependencies [969a88669]
  - @mysten/sui.js@0.25.0
  - @mysten/wallet-adapter-base@0.4.2

## 0.4.1

### Patch Changes

- Updated dependencies [01458ffd5]
- Updated dependencies [a274ecfc7]
- Updated dependencies [88a687834]
- Updated dependencies [89091ddab]
- Updated dependencies [71bee7563]
  - @mysten/sui.js@0.24.0
  - @mysten/wallet-adapter-base@0.4.1

## 0.4.0

### Minor Changes

- 65fd8ac17: - Support wallet adapter events

### Patch Changes

- Updated dependencies [f3444bdf2]
- Updated dependencies [e26f47cbf]
- Updated dependencies [b745cde24]
- Updated dependencies [01efa8bc6]
- Updated dependencies [35e0df780]
- Updated dependencies [65fd8ac17]
- Updated dependencies [5cd51dd38]
- Updated dependencies [8474242af]
- Updated dependencies [01efa8bc6]
- Updated dependencies [f74181212]
  - @mysten/sui.js@0.23.0
  - @mysten/wallet-adapter-base@0.4.0

## 0.3.4

### Patch Changes

- Updated dependencies [a55236e48]
- Updated dependencies [8ae226dae]
  - @mysten/sui.js@0.22.0
  - @mysten/wallet-adapter-base@0.3.9

## 0.3.3

### Patch Changes

- c8bab06b0: Introduce new framework-agnostic Wallet Kit Core package.
- Updated dependencies [4fb12ac6d]
- Updated dependencies [bb14ffdc5]
- Updated dependencies [9fbe2714b]
- Updated dependencies [d2015f815]
- Updated dependencies [7d0f25b61]
  - @mysten/sui.js@0.21.0
  - @mysten/wallet-adapter-base@0.3.8

## 0.3.2

### Patch Changes

- Updated dependencies [f93b59f3a]
- Updated dependencies [ea71d8216]
  - @mysten/sui.js@0.20.0
  - @mysten/wallet-adapter-base@0.3.7

## 0.3.1

### Patch Changes

- Updated dependencies [b8257cecb]
- Updated dependencies [6c1f81228]
- Updated dependencies [519e11551]
- Updated dependencies [b03bfaec2]
- Updated dependencies [f9be28a42]
- Updated dependencies [24987df35]
  - @mysten/sui.js@0.19.0
  - @mysten/wallet-adapter-base@0.3.6

## 0.3.0

### Minor Changes

- e6282ae71: Improve APIs and add missing icons.

### Patch Changes

- Updated dependencies [66021884e]
- Updated dependencies [7a67d61e2]
- Updated dependencies [45293b6ff]
- Updated dependencies [7a67d61e2]
- Updated dependencies [2a0b8e85d]
  - @mysten/sui.js@0.18.0
  - @mysten/wallet-adapter-base@0.3.5

## 0.2.3

### Patch Changes

- Updated dependencies [623505886]
  - @mysten/sui.js@0.17.1
  - @mysten/wallet-adapter-base@0.3.4

## 0.2.2

### Patch Changes

- Updated dependencies [a9602e533]
- Updated dependencies [db22728c1]
- Updated dependencies [3b510d0fc]
  - @mysten/sui.js@0.17.0
  - @mysten/wallet-adapter-base@0.3.3

## 0.2.1

### Patch Changes

- Updated dependencies [01989d3d5]
- Updated dependencies [5e20e6569]
  - @mysten/sui.js@0.16.0
  - @mysten/wallet-adapter-base@0.3.2

## 0.2.0

### Minor Changes

- f5679be35: Introduce unsafe burner wallet adapter

### Patch Changes

- Updated dependencies [c27933292]
- Updated dependencies [90898d366]
- Updated dependencies [c27933292]
- Updated dependencies [c27933292]
  - @mysten/sui.js@0.15.0
  - @mysten/wallet-adapter-base@0.3.1
