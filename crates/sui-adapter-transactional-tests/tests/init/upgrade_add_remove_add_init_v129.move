// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Old behavior (pre-`init`-on-upgrade): add-remove-add of an `init` on an existing module is
// permitted, and no `init` runs on upgrade.

//# init --protocol-version 129 --addresses v0=0x0 v1=0x0 v2=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::a;
fun init(_: &mut TxContext) { }

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::a;

//# upgrade --package v1 --upgrade-capability 1,1 --sender A
module v2::a;
fun init(_: &mut TxContext) {
    abort 0
}
