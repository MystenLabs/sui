// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tripwire for the deny-list address bound. Eleven allowance withdrawals exceed the reservation
// limit, so signing rejects the transaction before the deny-list check runs. If that limit is
// lifted or raised, the check sees twelve addresses and its debug assertion fires.

//# init --accounts A C F1 F2 F3 F4 F5 F6 F7 F8 F9 F10 F11 --addresses test=0x0

//# publish --sender A
#[allow(deprecated_usage)]
module test::regulated_coin;

use sui::coin;

public struct REGULATED_COIN has drop {}

fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency(
        otw,
        9,
        b"RC",
        b"REGULATED_COIN",
        b"A new regulated coin",
        option::none(),
        ctx,
    );
    let coin = coin::mint(&mut treasury_cap, 10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}

//# programmable --sender A --inputs object(1,5) 100 @F1 @F2 @F3 @F4 @F5 @F6 @F7 @F8 @F9 @F10 @F11
// Fund each funder's address balance.
//> 0: SplitCoins(Input(0), [Input(1), Input(1), Input(1), Input(1), Input(1), Input(1), Input(1), Input(1), Input(1), Input(1), Input(1)]);
//> 1: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,0), Input(2));
//> 2: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,1), Input(3));
//> 3: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,2), Input(4));
//> 4: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,3), Input(5));
//> 5: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,4), Input(6));
//> 6: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,5), Input(7));
//> 7: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,6), Input(8));
//> 8: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,7), Input(9));
//> 9: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,8), Input(10));
//> 10: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,9), Input(11));
//> 11: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,10), Input(12));

//# create-checkpoint

//# programmable --sender F1 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F1 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F2 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F2 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F3 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F3 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F4 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F4 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F5 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F5 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F6 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F6 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F7 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F7 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F8 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F8 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F9 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F9 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F10 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F10 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender F11 --inputs b"" @C vector[100u256] vector[] vector[99999999999999]
// F11 issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F1,object(4,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F2,object(5,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F3,object(6,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F4,object(7,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F5,object(8,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F6,object(9,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F7,object(10,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F8,object(11,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F9,object(12,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F10,object(13,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(10,@F11,object(14,0)) mutshared(4,0) mutshared(5,0) mutshared(6,0) mutshared(7,0) mutshared(8,0) mutshared(9,0) mutshared(10,0) mutshared(11,0) mutshared(12,0) mutshared(13,0) mutshared(14,0) immshared(6) @C
// Eleven withdrawals: one over the limit, rejected at signing.
//> 0: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(11), Input(0), Input(22));
//> 1: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(23));
//> 2: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(12), Input(1), Input(22));
//> 3: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(2), Input(23));
//> 4: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(13), Input(2), Input(22));
//> 5: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(4), Input(23));
//> 6: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(14), Input(3), Input(22));
//> 7: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(6), Input(23));
//> 8: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(15), Input(4), Input(22));
//> 9: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(8), Input(23));
//> 10: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(16), Input(5), Input(22));
//> 11: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(10), Input(23));
//> 12: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(17), Input(6), Input(22));
//> 13: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(12), Input(23));
//> 14: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(18), Input(7), Input(22));
//> 15: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(14), Input(23));
//> 16: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(19), Input(8), Input(22));
//> 17: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(16), Input(23));
//> 18: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(20), Input(9), Input(22));
//> 19: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(18), Input(23));
//> 20: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(21), Input(10), Input(22));
//> 21: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(20), Input(23));
