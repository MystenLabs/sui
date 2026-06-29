// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 126 --addresses v0=0x0 v1=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::a {
}

//# view-object 1,0

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::a {
    fun init(_: &mut TxContext) {
        abort 0
    }
}
