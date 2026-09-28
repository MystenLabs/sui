// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Signing checks every address a regulated coin is debited from, per coin type:
// the sender for everything, plus the funder of each allowance withdrawal.
// A: issuer, holds both deny caps. B, D: funders. C: spender and sender.

//# init --accounts A B C D --addresses test=0x0

//# publish --sender A
module test::regulated_coin1 {
    use sui::coin;

    public struct REGULATED_COIN1 has drop {}

    #[allow(deprecated_usage)]
    fun init(otw: REGULATED_COIN1, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 10000, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }
}

module test::regulated_coin2 {
    use sui::coin;

    public struct REGULATED_COIN2 has drop {}

    #[allow(deprecated_usage)]
    fun init(otw: REGULATED_COIN2, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 10000, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }
}

//# programmable --sender A --inputs object(1,5) object(1,10) 1000 @B @C @D
// Fund address balances: RC1 for B, C and D; RC2 for B.
//> 0: SplitCoins(Input(0), [Input(2), Input(2), Input(2)]);
//> 1: sui::coin::send_funds<test::regulated_coin1::REGULATED_COIN1>(NestedResult(0,0), Input(3));
//> 2: sui::coin::send_funds<test::regulated_coin1::REGULATED_COIN1>(NestedResult(0,1), Input(4));
//> 3: sui::coin::send_funds<test::regulated_coin1::REGULATED_COIN1>(NestedResult(0,2), Input(5));
//> 4: SplitCoins(Input(1), [Input(2)]);
//> 5: sui::coin::send_funds<test::regulated_coin2::REGULATED_COIN2>(Result(4), Input(3));

//# create-checkpoint

//# programmable --sender B --inputs b"" @C vector[1000u256] vector[] vector[99999999999999]
// B issues an RC1 allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --inputs b"" @C vector[1000u256] vector[] vector[99999999999999]
// B issues an RC2 allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin2::REGULATED_COIN2>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender D --inputs b"" @C vector[1000u256] vector[] vector[99999999999999]
// D issues an RC1 allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

// === Two funders of the same coin, one denied ===

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(4,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@D,object(6,0)) mutshared(4,0) mutshared(6,0) immshared(6) @C
// RC1 from B and from D in one transaction, nobody denied.
//> 0: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(2), Input(0), Input(4));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(5));
//> 2: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(3), Input(1), Input(4));
//> 3: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(2), Input(5));

//# run sui::coin::deny_list_add --args object(0x403) object(1,3) @D --type-args test::regulated_coin1::REGULATED_COIN1 --sender A

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(4,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@D,object(6,0)) mutshared(4,0) mutshared(6,0) immshared(6) @C
// The same transaction with D denied: rejected, naming D.
//> 0: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(2), Input(0), Input(4));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(5));
//> 2: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(3), Input(1), Input(4));
//> 3: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(2), Input(5));

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(4,0)) mutshared(4,0) immshared(6) @C
// Only B's withdrawal: D's denial does not touch it.
//> 0: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(3));

//# run sui::coin::deny_list_remove --args object(0x403) object(1,3) @D --type-args test::regulated_coin1::REGULATED_COIN1 --sender A

// === One funder, two coins, denied for one of them ===

//# run sui::coin::deny_list_add --args object(0x403) object(1,8) @B --type-args test::regulated_coin2::REGULATED_COIN2 --sender A

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(4,0)) mutshared(4,0) immshared(6) @C
// B is denied for RC2 only, so B's RC1 still spends.
//> 0: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(3));

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(4,0)) allowance_withdraw<sui::balance::Balance<test::regulated_coin2::REGULATED_COIN2>>(100,@B,object(5,0)) mutshared(4,0) mutshared(5,0) immshared(6) @C
// Adding B's RC2 to the same transaction: rejected, naming B for RC2.
//> 0: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(2), Input(0), Input(4));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(5));
//> 2: sui::allowance::balance_spend<test::regulated_coin2::REGULATED_COIN2>(Input(3), Input(1), Input(4));
//> 3: sui::balance::send_funds<test::regulated_coin2::REGULATED_COIN2>(Result(2), Input(5));

//# run sui::coin::deny_list_remove --args object(0x403) object(1,8) @B --type-args test::regulated_coin2::REGULATED_COIN2 --sender A

// === Sender denied, funder clear ===

//# run sui::coin::deny_list_add --args object(0x403) object(1,3) @C --type-args test::regulated_coin1::REGULATED_COIN1 --sender A

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(4,0)) mutshared(4,0) immshared(6) @C
// The funds are B's, but C directs the spend: rejected, naming C.
//> 0: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(3));

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin2::REGULATED_COIN2>>(100,@B,object(5,0)) mutshared(5,0) immshared(6) @C
// C is denied for RC1 only, so B's RC2 still spends.
//> 0: sui::allowance::balance_spend<test::regulated_coin2::REGULATED_COIN2>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::regulated_coin2::REGULATED_COIN2>(Result(0), Input(3));

//# run sui::coin::deny_list_remove --args object(0x403) object(1,3) @C --type-args test::regulated_coin1::REGULATED_COIN1 --sender A

// === Own withdrawal next to an allowance withdrawal ===

//# programmable --sender C --inputs withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100) allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(4,0)) mutshared(4,0) immshared(6) @C @A
// C's own RC1 goes to A and B's RC1 to C, nobody denied.
//> 0: sui::balance::redeem_funds<test::regulated_coin1::REGULATED_COIN1>(Input(0));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(5));
//> 2: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(2), Input(1), Input(3));
//> 3: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(2), Input(4));

//# run sui::coin::deny_list_add --args object(0x403) object(1,3) @B --type-args test::regulated_coin1::REGULATED_COIN1 --sender A

//# programmable --sender C --inputs withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100) allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(4,0)) mutshared(4,0) immshared(6) @C @A
// The same transaction with B denied: rejected on the allowance leg, naming B.
//> 0: sui::balance::redeem_funds<test::regulated_coin1::REGULATED_COIN1>(Input(0));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(5));
//> 2: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(2), Input(1), Input(3));
//> 3: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(2), Input(4));

//# programmable --sender C --inputs withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100) @A
// C's own RC1 alone still spends.
//> 0: sui::balance::redeem_funds<test::regulated_coin1::REGULATED_COIN1>(Input(0));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(1));

// === Denied before the allowance exists ===

//# programmable --sender B --inputs b"" @C vector[1000u256] vector[] vector[99999999999999]
// Issuing touches no regulated object, so B, still denied for RC1, can issue a
// second RC1 allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(24,0)) mutshared(24,0) immshared(6) @C
// Spending through it is rejected, naming B.
//> 0: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(3));

//# run sui::coin::deny_list_remove --args object(0x403) object(1,3) @B --type-args test::regulated_coin1::REGULATED_COIN1 --sender A

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin1::REGULATED_COIN1>>(100,@B,object(24,0)) mutshared(24,0) immshared(6) @C
// Once B is clear the new allowance spends like any other.
//> 0: sui::allowance::balance_spend<test::regulated_coin1::REGULATED_COIN1>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::regulated_coin1::REGULATED_COIN1>(Result(0), Input(3));
