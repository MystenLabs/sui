// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses v0=0x0 v1=0x0 v2=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::a {
    public(package) fun val_for_b(): u64 { abort 0 }
    public(package) fun val_for_c(): u64 { abort 0 }
}

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::a {
    public(package) fun val_for_b(): u64 { 0xb }
    public(package) fun val_for_c(): u64 { abort 0 }
    fun init(_: &mut TxContext) {
        // TODO this is not an error right now that this `init` was added
        abort 0
    }
}
module v1::b {
    public struct B has key {
        id: UID,
        v: u64,
    }
    fun init(ctx: &mut TxContext) {
        transfer::transfer(B { id: object::new(ctx), v: v1::a::val_for_b() }, ctx.sender());
    }
}

//# view-object 2,0

//# upgrade --package v1 --upgrade-capability 1,1 --sender A
module v2::a {
    public(package) fun val_for_b(): u64 { abort 0 }
    public(package) fun val_for_c(): u64 { 0xc }
}
module v2::b {
    public struct B has key {
        id: UID,
        v: u64,
    }
}
module v2::c {
    public struct C has key {
        id: UID,
        v: u64,
    }
    fun init(ctx: &mut TxContext) {
        transfer::transfer(C { id: object::new(ctx), v: v2::a::val_for_c() }, ctx.sender());
    }
}

//# view-object 4,0
