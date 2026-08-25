// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Old behavior (pre-`init`-on-upgrade): the same upgrade is rejected, but for a different reason --
// the new module with an `init` trips the "init in new modules on upgrade not yet supported" guard
// before the existing-module-adds-init rule (which only exists once the feature is enabled) is ever
// consulted.

//# init --protocol-version 129 --addresses v0=0x0 v1=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::a;

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::a {
    fun init(_: &mut TxContext) {
        abort 0
    }
}
module v1::z {
    fun init(_: &mut TxContext) {
        abort 0
    }
}
