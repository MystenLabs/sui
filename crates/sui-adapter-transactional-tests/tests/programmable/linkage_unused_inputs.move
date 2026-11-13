// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Unused owned objects and explicit funds withdrawals still impose type-package minimum
// versions. Tests that they raise a public consumer from dependency v1 to v2 but conflict
// with an exact v1 call target; the control without those inputs stays on v1.

//# init --addresses DepV1=0x0 DepV2=0x0 Consumer=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m {
    public fun version(): u64 { 1 }

    public fun ping() {}
}

//# publish --dependencies DepV1 --sender A
module Consumer::m {
    public fun check(expected: u64) { assert!(DepV1::m::version() == expected, 0) }
}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m {
    public struct NewObject has key, store { id: UID }
    public struct NewType has drop {}
    public struct Mint has key { id: UID, supply: sui::balance::Supply<NewType> }

    public fun fund(ctx: &mut TxContext) {
        let mut supply = sui::balance::create_supply(NewType {});
        sui::balance::send_funds(supply.increase_supply(2), ctx.sender());
        transfer::transfer(Mint { id: object::new(ctx), supply }, ctx.sender())
    }

    public fun version(): u64 { 2 }

    public fun ping() {}

    public fun make(ctx: &mut TxContext) {
        transfer::transfer(NewObject { id: object::new(ctx) }, ctx.sender())
    }
}

//# run DepV2::m::make --sender A

//# programmable --sender A --inputs 1u64
// Without another contribution, the public call uses DepV1.
//> Consumer::m::check(Input(0));

//# programmable --sender A --inputs object(4,0) 2u64
// The unused object was introduced in v2 and upgrades the public call's dependency.
//> Consumer::m::check(Input(1));

//# programmable --sender A --inputs object(4,0)
// The unused object's minimum v2 conflicts with an exact v1 call target.
//> DepV1::m::ping();

//# run DepV2::m::fund --sender A

//# create-checkpoint

//# programmable --sender A --inputs withdraw<sui::balance::Balance<DepV2::m::NewType>>(1) 2u64
// An unused explicit withdrawal also raises the public call's dependency to v2.
//> Consumer::m::check(Input(1));

//# programmable --sender A --inputs withdraw<sui::balance::Balance<DepV2::m::NewType>>(1)
// Its minimum v2 also conflicts with an exact v1 target.
//> DepV1::m::ping();
