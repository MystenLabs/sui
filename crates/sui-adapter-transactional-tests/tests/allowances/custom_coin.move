// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Allowances over a custom coin: the funder's only balance is FOO (reservation
// checking is per-type, no SUI required), and a dual-type PTB spends a FOO and
// a SUI allowance from the same funder as two independent keys.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
#[allow(deprecated_usage)]
module test::foo;

use sui::coin;

public struct FOO has drop {}

fun init(otw: FOO, ctx: &mut TxContext) {
    let (treasury_cap, metadata) = coin::create_currency(
        otw, 6, b"FOO", b"Foo", b"", option::none(), ctx,
    );
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury_cap, ctx.sender());
}

//# programmable --sender A --inputs object(1,1) 5000 @A
// Fund A's FOO address balance; A holds no SUI balance yet.
//> 0: sui::coin::mint<test::foo::FOO>(Input(0), Input(1));
//> 1: sui::coin::send_funds<test::foo::FOO>(Result(0), Input(2));

//# create-checkpoint

//# programmable --sender A --inputs b"foo" @B vector[1000u256] vector[] vector[99999999999999]
// A issues a FOO allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::foo::FOO>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<test::foo::FOO>>(400,@A,object(4,0)) mutshared(4,0) immshared(6) @B
// B spends 400 FOO while the funder has no SUI balance at all.
//> 0: sui::allowance::balance_spend<test::foo::FOO>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::foo::FOO>(Result(0), Input(3));

//# view-funds sui::balance::Balance<test::foo::FOO> A
// Still 5000: the spend settles at the checkpoint.

//# view-funds sui::balance::Balance<test::foo::FOO> B
// No accumulator object yet: the merge settles at the checkpoint.

//# create-checkpoint

//# view-funds sui::balance::Balance<test::foo::FOO> A

//# view-funds sui::balance::Balance<test::foo::FOO> B

//# programmable --sender A --inputs 1000 @A
// Fund A's SUI balance for the dual-type spend below.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"sui" @B vector[1000u256] vector[] vector[99999999999999]
// A issues a SUI allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<test::foo::FOO>>(300,@A,object(4,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(200,@A,object(13,0)) mutshared(4,0) mutshared(13,0) immshared(6) @B
// One PTB, two types from one funder: two keys, two Splits.
//> 0: sui::allowance::balance_spend<test::foo::FOO>(Input(2), Input(0), Input(4));
//> 1: sui::balance::send_funds<test::foo::FOO>(Result(0), Input(5));
//> 2: sui::allowance::balance_spend<sui::sui::SUI>(Input(3), Input(1), Input(4));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(5));

//# view-object 4,0
// The FOO allowance's current_spend is 700.

//# view-object 13,0
// The SUI allowance's current_spend is 200.

//# create-checkpoint

//# view-funds sui::balance::Balance<test::foo::FOO> A
// 5000 - 400 - 300 = 4300.

//# view-funds sui::balance::Balance<sui::sui::SUI> A
// 1000 - 200 = 800.

//# view-funds sui::balance::Balance<test::foo::FOO> B
// 400 + 300 = 700.

//# view-funds sui::balance::Balance<sui::sui::SUI> B
// 200.
