// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A shared object that can hold Balance<T> via the funds accumulator.
/// Used for stress testing object balance operations.
module basics::balance_pool {
	use sui::balance::{Self, Balance};

	public struct BalancePool has key {
		id: UID,
	}

	public fun create(ctx: &mut TxContext) {
		transfer::share_object(BalancePool {
			id: object::new(ctx),
		})
	}

	public fun deposit<T>(pool: &BalancePool, balance: Balance<T>) {
		balance::send_funds(balance, pool.id.to_address())
	}

	public fun withdraw<T>(
		pool: &mut BalancePool,
		value: u64,
	): sui::funds_accumulator::Withdrawal<Balance<T>> {
		balance::withdraw_funds_from_object(&mut pool.id, value)
	}
}
