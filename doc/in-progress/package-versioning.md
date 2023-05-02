---
title: Package Versioning
---

Sui smart contracts are organized into [upgradeable](./package-upgrades.md) packages and, as a result, multiple versions of any given package can exist on-chain. Before a package can be used on the chain, it is [published](move/debug-publish.md#publishing-a-package) at its first (a.k.a, original) version. A new version of a package is created whenever the package is [upgraded](./package-upgrades.md), and an upgrade always happens with respect to the version of the package immediately preceding the upgraded one in the versions history. In other words, an upgrade to the Nth version of a package is always based on the N-1th version of the package (this is enforced by the system) and one can never create the Nth version of the package based on a version that's smaller than N-1th version. For example a package can be upgraded from version 1 to 2 but once this happens only an upgrade from version 2 to 3 is possible and an upgrade from version 1 to 3 is forbidden.

Note that there is also a notion of versioning present in the [manifest](./move/manifest.md) files. You can observe it both in the [package section](./move/manifest.md#package-section) and in the [dependencies section](./move/manifest.md#dependencies-section). For example:
```toml
[package]
name = "some_pkg"
version = "1.0.0"

[dependencies]
another_pkg = { git = "https://github.com/another_pkg/another_pkg.git" , version = "2.0.0"}
```
These fields in the manifest file, however, at this point are only used for user-level documentation purposes and are ignored by the publish and upgrade commands. Another way of looking at it is that if a developer publishes a package with a certain package version in the manifest file and then modifies and re-publishes the same package with a different version (using publish command rather than upgrade command), these two packages will be considered different packages, rather than on-chain versions of the same package. For example, none of these packages should be used as a [dependency override](./dependency-overrides.md) to stand in for the other one - while this kind of override is possible to specify when building a package, it will result in an error when publishing/upgrading on-chain.

## Compatibility

Sui comes with a set of built-in package compatibility policies, listed here from most to least strict:

- immutable - this policy is a bit of a misnomer as having it applied means that the package cannot be upgraded at all
- dependency-only - the only thing that can be modified in an upgraded version of this package are its dependencies
- additive - new functionality can be added to the package (e.g., new public functions or structs) but none of the existing functionality can be changed (e.g., the code in existing public functions cannot change)
- compatible - the most relaxed policy where in addition to everything allowed by more restrictive policies, in an upgraded version of the package:
  - all function implementations can change.
  - ability constraints on generic type parameters in function signatures can be removed
  - `private`, `public(friend)`, and `entry` function signatured can be changed, they can also be removed, or made `public`.
  - `public` function signatures cannot be changed (except in the case of ability constraints as mentioned previously).
  - existing types cannot change

Note that each of these policies, in the order listed, is a superset of the previous one in what kind of changes is allowed in the upgraded package.

When a package is published, by default it adopts the most relaxed policy, that is the compatible one. Note that a package can be published as part of a transaction block that can change the policy before the transaction block is successfully completed, making the package available on-chain for the first time at the desired policy level rather than at the default one.

The current policy can be changed by calling one of the functions in `sui::package` (`only_additive_upgrades`, `only_dep_upgrades`, `make_immutable`) on the package's [`UpgradeCap`](./custom-upgrade-policy.md#upgradecap) and a policy can only become more restrictive. For example, once a developer restricts the policy to become additive by calling `sui::package::only_dep_upgrades`, calling ``sui::package::only_additive_upgrades` on the `UpgradeCap` of the same package will result in an error.
