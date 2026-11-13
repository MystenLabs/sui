// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The budget boundary of computation and of storage. Computation runs out of gas when its charge
// equals the budget. Storage fits when its net charge equals what computation left. Gas is paid
// from an address balance, so no gas coin storage is charged.

//# init --addresses test=0x0 --accounts A

//# publish
module test::boundary;

public struct W has key, store { id: UID }

public fun noop() {}

public fun make(ctx: &mut TxContext) {
    sui::transfer::public_transfer(W { id: object::new(ctx) }, ctx.sender());
}

//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// The minimum computation of 1_000 units costs 1_000_000 MIST at gas price 1_000. It equals the
// budget: out of gas, and the 1_000_000 MIST are charged.
//# programmable --sender A --address-balance-gas --gas-budget 1000000
//> test::boundary::noop()

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// One more MIST fits. The VM gets the same 1_000 units, and the charge is 1_000_000 MIST.
//# programmable --sender A --address-balance-gas --gas-budget 1000001
//> test::boundary::noop()

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Computation of 1_000_000 MIST and the storage of one W use the whole budget, and storage fits.
//# programmable --sender A --address-balance-gas --gas-budget 2261600
//> test::boundary::make()

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// One MIST less: storage does not fit. The W is dropped and only computation is charged.
//# programmable --sender A --address-balance-gas --gas-budget 2261599
//> test::boundary::make()

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
