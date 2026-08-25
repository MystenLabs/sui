// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// An existing module (present in the prior version) that did not define an `init` may not add one
// during an upgrade: doing so is rejected as an incompatible upgrade.

//# init --addresses v0=0x0 v1=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::a {
}

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::a {
    fun init(_: &mut TxContext) {
        abort 0
    }
}
