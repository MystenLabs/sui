# @mysten/kiosk

## 0.3.3

### Patch Changes

- Updated dependencies [34cc7d610]
  - @mysten/sui.js@0.37.1

## 0.3.2

### Patch Changes

- Updated dependencies [36f2edff3]
- Updated dependencies [75d1a190d]
- Updated dependencies [93794f9f2]
- Updated dependencies [c3a4ec57c]
- Updated dependencies [a17d3678a]
- Updated dependencies [2f37537d5]
- Updated dependencies [00484bcc3]
  - @mysten/sui.js@0.37.0

## 0.3.1

### Patch Changes

- 6a2a42d779: Add `getOwnedKiosks` query to easily get owned kiosks and their ownerCaps for an address
- abf6ad381e: Refactor the fetchKiosk function to return all content instead of paginating, to prevent missing data
- d72fdb5a5c: Fix on createTransferPolicy method. Updated type arguments for public_share_object command.
- Updated dependencies [3ea9adb71a]
- Updated dependencies [1cfb1c9da3]
- Updated dependencies [1cfb1c9da3]
- Updated dependencies [fb3bb9118a]
  - @mysten/sui.js@0.36.0

## 0.3.0

### Minor Changes

- 968304368d: Support kiosk_lock_rule and environment support for rules package. Breaks `purchaseAndResolvePolicies` as it changes signature and return format.

### Patch Changes

- Updated dependencies [09d77325a9]
  - @mysten/sui.js@0.35.1

## 0.2.0

### Minor Changes

- c322a230da: Fix fetchKiosk consistency/naming, include locked state in items

## 0.1.0

### Minor Changes

- 4ea96d909a: Kiosk SDK for managing, querying and interacting with Kiosk and TransferPolicy objects

### Patch Changes

- 528cfec314: fixes publishing flow
- Updated dependencies [4ea96d909a]
- Updated dependencies [bcbb178c44]
- Updated dependencies [470c27af50]
- Updated dependencies [03828224c9]
- Updated dependencies [671faefe3c]
- Updated dependencies [9ce7e051b4]
- Updated dependencies [9ce7e051b4]
- Updated dependencies [bb50698551]
  - @mysten/sui.js@0.35.0
