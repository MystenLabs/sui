// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test demonstrating arithmetic overflow protection when withdrawing from address balances.
// Creates two independent Supply objects, mints 18446744073709551614 (u64::MAX - 1) from each,
// sends both amounts to address A, then attempts to withdraw both amounts in a single PTB.
// The withdraw operation fails with arithmetic error because the total exceeds u64::MAX.

//# init --addresses test=0x0 --accounts A B --enable-accumulators --simulator

//# publish --sender A
module test::large_balance {
    use sui::balance::{Self, Supply};

    public struct MARKER has drop {}

    public struct SupplyHolder has key, store {
        id: UID,
        supply: Supply<MARKER>,
    }

    public fun create_holder(ctx: &mut TxContext): SupplyHolder {
        SupplyHolder {
            id: object::new(ctx),
            supply: balance::create_supply(MARKER {}),
        }
    }

    public fun send_large_balance(holder: &mut SupplyHolder, recipient: address, amount: u64) {
        let balance = holder.supply.increase_supply(amount);
        balance::send_funds(balance, recipient);
    }
}

//# programmable --sender A --inputs @A
//> 0: test::large_balance::create_holder();
//> 1: test::large_balance::create_holder();
//> TransferObjects([Result(0), Result(1)], Input(0))

//# run test::large_balance::send_large_balance --args object(2,0) @A 18446744073709551614 --sender A

//# create-checkpoint

//# run test::large_balance::send_large_balance --args object(2,1) @A 18446744073709551614 --sender A

//# create-checkpoint

//# programmable --sender A --inputs withdraw<sui::balance::Balance<test::large_balance::MARKER>>(18446744073709551614) withdraw<sui::balance::Balance<test::large_balance::MARKER>>(18446744073709551614) @B
//> 0: sui::balance::redeem_funds<test::large_balance::MARKER>(Input(0));
//> 1: sui::balance::redeem_funds<test::large_balance::MARKER>(Input(1));
//> 2: sui::balance::join<test::large_balance::MARKER>(Result(0), Result(1));
//> 3: sui::balance::send_funds<test::large_balance::MARKER>(Result(0), Input(2));
