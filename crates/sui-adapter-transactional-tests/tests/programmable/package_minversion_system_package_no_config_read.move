// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Minversion selection applies to user-package families, not framework packages. This transaction
// calls `sui::package` and passes an `UpgradeCap`; both the function and the argument type belong to
// system packages. It must therefore execute without reading the PackageConfig shared object. The
// snapshot proves this by showing no `unchanged_shared` entry for 0xcf6.

//# init --addresses User=0x0 --accounts A

//# publish --upgradeable --sender A
module User::user;
public fun ping() {}

// The call root and its argument type are both from system packages. There is no implicit
// PackageConfig read, so the effects must not contain unchanged_shared 0xcf6.
//# run sui::package::only_additive_upgrades --args object(1,1) --sender A
