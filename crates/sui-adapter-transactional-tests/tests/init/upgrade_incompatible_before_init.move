// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests the compatibility is run before init, otherwise some strange things can happen with
// type layout

//# init --addresses v0=0x0 v1=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::m {
    public struct Obj has key, store {
        id: UID,
        f: u64,
    }
}

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::m {
    public struct Obj has key, store {
        id: UID,
        f: u64,
        g: u64,
    }

    public fun new(ctx: &mut TxContext): Obj {
        Obj { id: object::new(ctx), f: std::u64::max_value!(), g: std::u64::max_value!() }
    }
}
module v1::n {
    fun init(ctx: &mut TxContext) {
        transfer::public_transfer(v1::m::new(ctx), ctx.sender());
    }
}
