// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Binding violations die at transaction input validation: a sender who is not
// the spender, and a declared funds type that doesn't match the allowance's.

//# init --accounts A B C --addresses test=0x0

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

//# programmable --sender A --inputs 5000 @A
// Fund A's (the funder) SUI address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender A --inputs object(1,1) 500 @A
// Fund A's FOO address balance, so the type-mismatch case below gets past the
// balance-availability check and reaches the allowance checks.
//> 0: sui::coin::mint<test::foo::FOO>(Input(0), Input(1));
//> 1: sui::coin::send_funds<test::foo::FOO>(Result(0), Input(2));

//# create-checkpoint

//# programmable --sender A --inputs b"txn_test" @B vector[1000u256] vector[] vector[99999999999999] vector[] vector[]
// A issues a Balance<SUI> allowance to B: 1000 lifetime cap.
//> 0: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Input(5), Input(6));

//# view-object 5,0

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@A,object(5,0)) mutshared(5,0) immshared(6)
// C is not the spender: rejected at signing.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(1), Input(0), Input(2));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<test::foo::FOO>>(100,@A,object(5,0)) mutshared(5,0) immshared(6)
// B declares a Balance<FOO> withdrawal against the Balance<SUI> allowance:
// rejected at signing on the funds type, not on FOO availability.
//> 0: sui::allowance::spend_balance<test::foo::FOO>(Input(1), Input(0), Input(2));

//# view-object 5,0
// current_spend is untouched by either attempt.
