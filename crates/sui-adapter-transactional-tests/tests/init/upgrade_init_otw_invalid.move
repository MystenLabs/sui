// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests the that the OTW witness verifier is run for new modules on
// upgrades

//# init --addresses v0=0x0 v1=0x0 --accounts A --flavor core

//# publish --upgradeable --sender A
module v0::m {
}

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::m {
}
module v1::has_otw {
    public struct HAS_OTW has drop {
    }
    fun init(_: &mut sui::tx_context::TxContext) {}
}
