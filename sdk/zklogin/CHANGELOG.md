# @mysten/zklogin

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
