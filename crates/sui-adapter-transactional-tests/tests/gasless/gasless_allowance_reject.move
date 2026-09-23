// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Gasless allowance spends rejected at signing. Signing checks do not run under the simulator,
// so these live apart from gasless_allowance.move.

//# init --addresses test=0x0 --accounts A B C --enable-gasless

//# publish --sender A
#[allow(deprecated_usage)]
module test::usdc {
    use sui::coin;

    public struct USDC has drop {}

    fun init(otw: USDC, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(
            otw, 6, b"USDC", b"USD Coin", b"", option::none(), ctx,
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, ctx.sender());
    }
}

//# gasless-allow-token test::usdc::USDC

//# programmable --sender A --inputs 10000 object(1,1) @A
// Mint 10000 USDC to A's address balance.
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 1: sui::coin::into_balance<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));

//# create-checkpoint

//# programmable --sender A --inputs b"b" @B vector[5000u256] vector[] vector[] 86400000 1000u256
// A issues B an allowance: 5000 lifetime cap, 1000 per day.
//> 0: sui::allowance::periodic_rate_limit(Input(5), Input(6));
//> 1: std::option::some<sui::allowance::RateLimit>(Result(0));
//> 2: sui::allowance::new<sui::balance::Balance<test::usdc::USDC>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(1));

//# programmable --sender B --address-balance-gas --gas-price 0 --gas-budget 0 --inputs allowance_withdraw<sui::balance::Balance<test::usdc::USDC>>(100,@A,object(5,0)) mutshared(5,0) mutshared(6) @C
// Rejected: the Clock must be an immutable input.
//> 0: sui::allowance::balance_spend<test::usdc::USDC>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(3));

//# programmable --sender B --address-balance-gas --gas-price 0 --gas-budget 0 --inputs allowance_withdraw<sui::balance::Balance<test::usdc::USDC>>(100,@A,object(5,0)) immshared(5,0) immshared(6) @C
// Rejected: the allowance must be a mutable input.
//> 0: sui::allowance::balance_spend<test::usdc::USDC>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(3));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs mutshared(5,0) object(5,1)
// Rejected: revoke is not a gasless function, and the allowance backs no withdrawal.
//> 0: sui::allowance::revoke<sui::balance::Balance<test::usdc::USDC>>(Input(1), Input(0));

//# programmable --sender A --inputs 5000 @A
// Fund A with SUI for a SUI allowance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"sui" @B vector[1000u256] vector[] vector[99999999999999]
// A SUI allowance for B.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --address-balance-gas --gas-price 0 --gas-budget 0 --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@A,object(11,0)) mutshared(11,0) immshared(6) @C
// Rejected: SUI is not an allowlisted gasless token.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));
