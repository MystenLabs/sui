// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Same as `upgrade_add_init_existing_and_new`, but the new init module (`a`) sorts before the
// offending existing module (`z`). 

//# init --addresses v0=0x0 v1=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::z;

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
