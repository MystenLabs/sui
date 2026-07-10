// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// An upgrade that both adds an `init` to an existing module and introduces a new module with an
// `init` must still be rejected: the error comes from the existing module gaining an `init` and is
// not masked by the presence of a new init module. Here the offending existing module (`a`) sorts
// before the new init module (`z`).

//# init --addresses v0=0x0 v1=0x0 --accounts A

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
