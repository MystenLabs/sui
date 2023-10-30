# @mysten/zklogin

## 0.3.3

### Patch Changes

- Updated dependencies [b9afb5567]
  - @mysten/sui.js@0.45.0

## 0.3.2

### Patch Changes

- c34c3c734: Revert additional JWT checks

## 0.3.1

### Patch Changes

- 4ba17833c: Fixes ESM usage of the SDK.

## 0.3.0

### Minor Changes

- 28ee0ff2f: Fix bug in nonce length check

## 0.2.1

### Patch Changes

- 9a1c8105e: Fix usage of string values in the SDK

## 0.2.0

### Minor Changes

- d80a6ed62: Remove toBigIntBE, expose new `getExtendedEphemeralPublicKey` method. Methods now return base64-encoded strings instead of bigints.

### Patch Changes

- 067d464f4: Introduce precise key-value pair parsing that matches the circuit
- b48289346: Mark packages as being side-effect free.
- Updated dependencies [b48289346]
- Updated dependencies [11cf4e68b]
  - @mysten/sui.js@0.44.0
  - @mysten/bcs@0.8.1

## 0.1.8

### Patch Changes

- Updated dependencies [004fb1991]
  - @mysten/sui.js@0.43.3

## 0.1.7

### Patch Changes

- Updated dependencies [9b052166d]
  - @mysten/sui.js@0.43.2

## 0.1.6

### Patch Changes

- c5684bb52: rename zk to zkLogin
- Updated dependencies [faa13ded9]
- Updated dependencies [c5684bb52]
  - @mysten/sui.js@0.43.1

## 0.1.5

### Patch Changes

- 3764c464f: - use new zklogin package from @mysten/sui.js for some of the zklogin functionality
  - rename `getZkSignature` to `getZkLoginSignature`
- 71e0a3197: - stop exporting `ZkSignatureInputs`
  - use `toBigEndianBytes` instead of `toBufferBE` that was renamed
- Updated dependencies [781d073d9]
- Updated dependencies [3764c464f]
- Updated dependencies [1bc430161]
- Updated dependencies [e4484852b]
- Updated dependencies [e4484852b]
- Updated dependencies [71e0a3197]
- Updated dependencies [1bc430161]
  - @mysten/sui.js@0.43.0
  - @mysten/bcs@0.8.0

## 0.1.4

### Patch Changes

- 9b3ffc7d6: - removes `AddressParams` bcs struct, now address is created by using the iss bytes
  - updated zklogin signature bcs struct for new camelCase fields
- d257d20ee: Improve nodejs compatibility
- Updated dependencies [fd8589806]
  - @mysten/sui.js@0.42.0

## 0.1.3

### Patch Changes

- 1786c68b5: Update hashASCIIStr logic and constants
- 8384490bb: Remove BCS export and introduce new getZkSignature export.
- 35bdbd00d: update bcs AddressParams struct
- 1f87936fd: Move aud to inner hash
- d89fff405: Expose new randomness function
- Updated dependencies [290c8e640]
  - @mysten/bcs@0.7.4
  - @mysten/sui.js@0.41.2

## 0.1.2

### Patch Changes

- d0750ea0f: rename pin to salt
- a82600f2d: fix nonce calculation
- Updated dependencies [24c21e1f0]
  - @mysten/sui.js@0.41.1

## 0.1.1

### Patch Changes

- b676fa4e9: Change function signature of genAddrSeed
- 626098033: Fix generated types
- 608e8e407: Update max key claim value length
- 5d344399f: Initial experimental zklogin SDK
- f5d5a4e8b: Polish utils
- f53d5823a: Change build process to use new internal build process.
