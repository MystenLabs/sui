# Changelog

## 0.2.3 (2024-05-02)

- Defender: Add `txOverrides` option. ([#49](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades/pull/49))

## 0.2.2 (2024-04-17)

- Defender: Fix handling of license types for block explorer verification, support `licenseType` and `skipLicenseType` options. ([#43](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades/pull/43))

## 0.2.1 (2024-03-20)

- Throw helpful error message if AST not found in contract artifacts. ([#28](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades/pull/28))

## 0.2.0 (2024-03-20)

- Update forge-std to v1.8.0, restrict state mutability of some functions. ([#30](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades/pull/30))

### Breaking changes
- Requires forge-std version v1.8.0 or later.

## 0.1.0 (2024-03-11)

- Support private networks and forked networks with Defender. ([#25](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades/pull/25))

## 0.0.2 (2024-02-20)

- Support constructor arguments for Defender deployments. ([#16](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades/pull/16))
- Support Defender deployments for upgradeable contracts. ([#18](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades/pull/18))
- Add `Defender.proposeUpgrade` function. ([#21](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades/pull/21))
- Add functions to get approval process information from Defender ([#23](https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades/pull/23))

### Breaking changes
- `Defender.deployContract` functions now return `address` instead of `string`.
- Defender deployments now require metadata to be included in compiler output.
- Defender deployments no longer print console output on successful deployments.

## 0.0.1 (2024-02-06)

- Initial preview