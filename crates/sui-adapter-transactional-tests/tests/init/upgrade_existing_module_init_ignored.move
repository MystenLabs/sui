// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Existing modules must not have init called during upgrade when they already had one. This also
// covers that an init in a module introduced by an earlier upgrade is not rerun on a later upgrade.

//# init --addresses v0=0x0 v1=0x0 v2=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::a {
    fun init(_: &mut TxContext) {
    }
}

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::a {
    fun init(_: &mut TxContext) {
        abort 0
    }
}
module v1::b {
    public struct B has key {
        id: UID,
        v: u64,
    }
    fun init(ctx: &mut TxContext) {
        transfer::transfer(B { id: object::new(ctx), v: 1 }, ctx.sender());
    }
}

//# view-object 2,0

//# view-object 2,1

//# upgrade --package v1 --upgrade-capability 1,1 --sender A
module v2::a {
}
module v2::b {
    public struct B has key {
        id: UID,
        v: u64,
    }
    fun init(_: &mut TxContext) {
        abort 1
    }
}

//# view-object 5,0
