// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses V0=0x0 V1=0x0 V2=0x0 V3=0x0 --accounts A

//# publish --upgradeable --sender A
module V0::base_module {
    public struct Object has key, store {
        id: UID,
        field0: u64,
        field1: u64,
    }

    public(package) entry fun friend_fun(): u64 { 0 }
}
module V0::friend_module {
    public fun call_friend(): u64 { V0::base_module::friend_fun() }
}

// Change the friend function signature -- should work
//# upgrade --package V0 --upgrade-capability 1,1 --sender A
module V1::base_module {
    public struct Object has key, store {
        id: UID,
        field0: u64,
        field1: u64,
    }

    public(package) entry fun friend_fun(x: u64): u64 { x }
}
module V1::friend_module {
    public fun call_friend(): u64 { V1::base_module::friend_fun(10) }
}

// Change the friend entry to a friend -- should work
//# upgrade --package V1 --upgrade-capability 1,1 --sender A
module V2::base_module {
    public struct Object has key, store {
        id: UID,
        field0: u64,
        field1: u64,
    }

    public fun public_fun(x: u64, recipient: address, ctx: &mut TxContext): u64 {
        transfer::public_transfer(
            Object { id: object::new(ctx), field0: x, field1: x},
            recipient
        );
        x
    }

    public fun public_fun2(x: u64): u64 { x }

    public(package) fun friend_fun(x: u64): u64 { x }
}
module V2::friend_module {
    public fun call_friend(): u64 { V1::base_module::friend_fun(10) }
}

// Remove a friend function -- and replace with a call to a public function at a previous version should also be fine
//# upgrade --package V2 --upgrade-capability 1,1 --sender A
module V3::base_module {
    public struct Object has key, store {
        id: UID,
        field0: u64,
        field1: u64,
    }

    public fun public_fun(x: u64, recipient: address, ctx: &mut TxContext): u64 {
        transfer::public_transfer(
            Object { id: object::new(ctx), field0: x, field1: x},
            recipient
        );
        x
    }
    public fun public_fun2(x: u64): u64 { x }
}
module V3::friend_module {
    public fun call_friend(): u64 { V2::base_module::public_fun2(10) }

    // Cross-version package call
    public fun call_public(ctx: &mut TxContext): u64 { V2::base_module::public_fun(10, @A, ctx) }
}

//# run V3::friend_module::call_friend

//# run V3::friend_module::call_public

//# view-object 6,0
