# Changelog

## 0.1.0 (2024-03-11)

- Support private networks and forked networks with Defender.

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