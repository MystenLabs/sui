// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Unused withdrawals whose type belongs to an upgraded package count as linkage participation
// and require initializer dependencies to match existing selections. Tests conflict precedence
// over later instantiation errors and a matching committed upgrade that does not consume
// the unused withdrawal's funds.

//# init --addresses DepV1=0x0 DepV2=0x0 Consumer=0x0 U1=0x0 U2=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m {
    public struct Box<phantom T> has drop {}

    public fun check(expected: u64) { assert!(expected == 1, 0) }

    public fun version(): u64 { 1 }
}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m {
    public struct Box<phantom T> has drop {}

    public fun check(expected: u64) { assert!(expected == 2, 0) }

    public fun version(): u64 { 2 }
}

//# publish --dependencies DepV1 --sender A
module Consumer::m {
    public fun check(expected: u64) { DepV1::m::check(expected) }

    public fun generic<T>() {}
}

//# publish --upgradeable --dependencies DepV1 --sender A
module U1::m {
    public struct Token has drop {}
    public struct Mint has key { id: UID, supply: sui::balance::Supply<Token> }

    public fun ping() { DepV1::m::check(1) }

    public fun fund(ctx: &mut TxContext) {
        let mut supply = sui::balance::create_supply(Token {});
        sui::balance::send_funds(supply.increase_supply(10), ctx.sender());
        transfer::transfer(Mint { id: object::new(ctx), supply }, ctx.sender())
    }
}

//# run U1::m::fund --sender A

//# create-checkpoint

//# stage-package --dependencies DepV2
module U2::m {
    public struct Token has drop {}
    public struct Mint has key { id: UID, supply: sui::balance::Supply<Token> }

    public fun ping() { DepV1::m::check(1) }

    public fun fund(ctx: &mut TxContext) {
        let mut supply = sui::balance::create_supply(Token {});
        sui::balance::send_funds(supply.increase_supply(10), ctx.sender());
        transfer::transfer(Mint { id: object::new(ctx), supply }, ctx.sender())
    }
}
module U2::new_init {
    public struct Config has key { id: UID, version: u64 }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Config { id: object::new(ctx), version: DepV1::m::version() })
    }
}

//# programmable --sender A --inputs object(4,1) 0u8 digest(U2) withdraw<sui::balance::Balance<U1::m::Token>>(1) 1u64
// The unused withdrawal alone makes U participate; init cannot replace the selected Dep v1.
//> Consumer::m::check(Input(4));
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(U2, [DepV2,sui,std], U1, Result(1));
//> sui::package::commit_upgrade(Input(0), Result(2));

//# programmable --sender A --inputs object(4,1) 0u8 digest(U2) withdraw<sui::balance::Balance<U1::m::Token>>(1) 1u64
// Deferred upgrade-init conflict precedes the later command's invalid instantiated type arity.
//> Consumer::m::check(Input(4));
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(U2, [DepV2,sui,std], U1, Result(1));
//> Consumer::m::generic<DepV1::m::Box<u64, u64>>();
//> sui::package::commit_upgrade(Input(0), Result(2));

//# programmable --sender A --inputs object(4,1) 0u8 digest(U2) withdraw<sui::balance::Balance<U1::m::Token>>(1) 2u64
// Participation succeeds when ordinary contributions already select init's required v2.
//> Consumer::m::check(Input(4));
//> DepV2::m::check(Input(4));
//> 2: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 3: Upgrade(U2, [DepV2,sui,std], U1, Result(2));
//> sui::package::commit_upgrade(Input(0), Result(3));

//# view-object 10,1
// The committed initializer observed version 2.

//# view-funds sui::balance::Balance<U1::m::Token> A
// Merely participating through an unused withdrawal did not consume its ten units.
