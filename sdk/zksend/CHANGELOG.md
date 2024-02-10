# @mysten/zksend

## 0.2.2

### Patch Changes

- 4830361fa4: Updated typescript version
- 4fd676671b: Fix issue with overwriting balances when adding multiple balances for the same unnormalized coinType"
- Updated dependencies [4830361fa4]
  - @mysten/wallet-standard@0.10.3
  - @mysten/sui.js@0.50.1

## 0.2.1

### Patch Changes

- f069e3a13d: fix listing assets for empty links

## 0.2.0

### Minor Changes

- e81f49e8dc: Add SDK for creating ZKSend links

### Patch Changes

- c07aa19958: Fix coin merging for sending balances
- 13e922d9b1: Rework timing and window opening logic to try and improve browser compatibility
- c859f41a1c: Handle base64 with spaces in hash
- d21c01ed47: Add method for claiming zksend assets from link
- 2814db6529: Fix required redirect
- e87d99734a: Add method for sending non-sui balances
- ba6fccd010: Add support for autoconnection from redirects
- c6b3066069: Fix cursor when enumerating links owned assets
- 66fbbc7faa: Detect gasCoin when claiming
- 7b8d044603: Detect wallet closing
- c6b3066069: Improve zkSend error messages
- a2904e0075: Fix for claimable assets not accounting for cases where claimable balance comes from gas coin
- ea2744b0c3: Add redirect parameter and fix listing assets on links without Sui
- 44a1f9ea0b: Tweak types of events sent over the bridge
- 7cc09a7bb4: Handle cases where list of objects to transfer is empty
- 9a14e61db4: Add gas estimation for creating zksend links
- f041b10b9f: Allow origin to be set when registering zksend wallet"
- c1f6cfff47: Fix import paths
- 7c9a8cc24b: Fix window opening for transactions with unresolved data
- ae9ae17eea: Fix ownedAfterClaim check
- Updated dependencies [a34f1cb67d]
- Updated dependencies [c08e3569ef]
- Updated dependencies [9a14e61db4]
- Updated dependencies [13e922d9b1]
- Updated dependencies [a34f1cb67d]
- Updated dependencies [220a766d86]
  - @mysten/sui.js@0.50.0
  - @mysten/wallet-standard@0.10.2

## 0.1.1

### Patch Changes

- Updated dependencies [9ac0a4ec01]
  - @mysten/wallet-standard@0.10.1

## 0.1.0

### Minor Changes

- e5f9e3ba21: Replace tsup based build to fix issues with esm/cjs dual publishing

### Patch Changes

- Updated dependencies [e5f9e3ba21]
  - @mysten/wallet-standard@0.10.0

## 0.0.3

### Patch Changes

- Updated dependencies [dd362ec1d6]
- Updated dependencies [165ad6b21d]
  - @mysten/wallet-standard@0.9.0

## 0.0.2

### Patch Changes

- @mysten/wallet-standard@0.8.11
