# @mysten/zksend

## 0.7.2

### Patch Changes

- Updated dependencies [99b112178c]
  - @mysten/sui.js@0.54.1
  - @mysten/wallet-standard@0.11.6

## 0.7.1

### Patch Changes

- 29d8e45b0e: Add support for sponsoring reclaimed links
- b7f673dbd9: Update graphql schema imports
- Updated dependencies [b7f673dbd9]
- Updated dependencies [123b42c75c]
  - @mysten/sui.js@0.54.0
  - @mysten/wallet-standard@0.11.5

## 0.7.0

### Minor Changes

- 879421a5ac: Add support for creating links from objects created in the same transaction block

### Patch Changes

- Updated dependencies [774bfb41a8]
  - @mysten/sui.js@0.53.0
  - @mysten/wallet-standard@0.11.4

## 0.6.0

### Minor Changes

- 905bc99e88: Add `createSendToAddressTransaction` method

### Patch Changes

- 9a9ff3cde1: Fix assets extracted from transactionBlocks using type instead of objectType from object changes"

## 0.5.0

### Minor Changes

- 44ba140be0: Add `getSentTransactionBlocksWithLinks` helper

## 0.4.4

### Patch Changes

- Updated dependencies [929db4976a]
  - @mysten/sui.js@0.52.0
  - @mysten/wallet-standard@0.11.3

## 0.4.3

### Patch Changes

- 59168f73ff: Make contract optional in ZkLSendLinkBuilder.createLinks
- b8f2a859ce: Handle the fetch function for environments that don't support window

## 0.4.2

### Patch Changes

- Updated dependencies [b4ecdb5860]
  - @mysten/sui.js@0.51.2
  - @mysten/wallet-standard@0.11.2

## 0.4.1

### Patch Changes

- Updated dependencies [6984dd1e38]
  - @mysten/sui.js@0.51.1
  - @mysten/wallet-standard@0.11.1

## 0.4.0

### Minor Changes

- c05a4e8cb7: removed listClaimableAssets, and added new assets and claimed properties to link instances
- c05a4e8cb7: Use contract by default for new links
- c05a4e8cb7: Add helper for bulk link creation
- c05a4e8cb7: Removed options for filtering claims
- c05a4e8cb7: renamed loadOwnedData to loadAssets

## 0.3.1

### Patch Changes

- b82832279b: Add isClaimTransaction helper

## 0.3.0

### Minor Changes

- 3b1da3967a: Add support for contract based links

## 0.2.3

### Patch Changes

- Updated dependencies [0cafa94027]
- Updated dependencies [437f0ca2ef]
  - @mysten/sui.js@0.51.0
  - @mysten/wallet-standard@0.11.0

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
