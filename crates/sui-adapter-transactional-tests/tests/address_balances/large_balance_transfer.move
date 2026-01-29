// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test demonstrating arithmetic overflow protection when withdrawing from address balances.
// Creates two independent Supply objects, mints 18446744073709551615 (u64::MAX) from each,
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

// Create two supply holders
//# programmable --sender A --inputs @A
//> 0: test::large_balance::create_holder();
//> 1: test::large_balance::create_holder();
//> TransferObjects([Result(0), Result(1)], Input(0))

// Send two large transfers in a single PTB - should cause Move abort due to overflow
//# programmable --sender A --inputs object(2,0) object(2,1) @A 18446744073709551615
//> 0: test::large_balance::send_large_balance(Input(0), Input(2), Input(3));
//> 1: test::large_balance::send_large_balance(Input(1), Input(2), Input(3));

//# create-checkpoint

// Send first large amount separately - should succeed
//# run test::large_balance::send_large_balance --args object(2,0) @A 18446744073709551615 --sender A

//# create-checkpoint

// Send second large amount separately - should succeed
//# run test::large_balance::send_large_balance --args object(2,1) @A 18446744073709551615 --sender A

//# create-checkpoint

// Withdraw first large amount - should succeed
//# programmable --sender A --inputs withdraw<sui::balance::Balance<test::large_balance::MARKER>>(18446744073709551615) @B
//> 0: sui::balance::redeem_funds<test::large_balance::MARKER>(Input(0));
//> 1: sui::balance::send_funds<test::large_balance::MARKER>(Result(0), Input(1));

//# create-checkpoint

// Withdraw second large amount - should succeed
//# programmable --sender A --inputs withdraw<sui::balance::Balance<test::large_balance::MARKER>>(18446744073709551615) @B
//> 0: sui::balance::redeem_funds<test::large_balance::MARKER>(Input(0));
//> 1: sui::balance::send_funds<test::large_balance::MARKER>(Result(0), Input(1));

//# create-checkpoint

// Attempt to withdraw both large amounts in a single PTB - should fail with arithmetic error
//# programmable --sender A --inputs withdraw<sui::balance::Balance<test::large_balance::MARKER>>(18446744073709551615) withdraw<sui::balance::Balance<test::large_balance::MARKER>>(18446744073709551615) @B
//> 0: sui::balance::redeem_funds<test::large_balance::MARKER>(Input(0));
//> 1: sui::balance::redeem_funds<test::large_balance::MARKER>(Input(1));
//> 2: sui::balance::join<test::large_balance::MARKER>(Result(0), Result(1));
//> 3: sui::balance::send_funds<test::large_balance::MARKER>(Result(0), Input(2));
