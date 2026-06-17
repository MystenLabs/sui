// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Unsponsored tx, sender A pays with `[AB(A), Coin]`, workload sends Gas to
// a random third party (C) and also creates a large object that blows
// storage. On OOG, reset drops execution writes (including the send_funds
// transfer) and re-smash undoes the override. The gas charge falls back to
// A's AB (the original smash-target location); C is not charged.

//# init --addresses test=0x0 --accounts A B C --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

//# publish
module test::big;
public struct W has key, store { id: UID, data: vector<u8> }
public fun make(size: u64, ctx: &mut TxContext) {
    let mut data = vector[];
    let mut i = 0;
    while (i < size) { vector::push_back(&mut data, 0u8); i = i + 1 };
    sui::transfer::public_transfer(W { id: object::new(ctx), data }, ctx.sender());
}

// Seed A's address balance.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Create a 1B secondary coin owned by A.
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// Pay with [AB(500M), Coin(1B)]; workload makes a big W then send_funds(Gas, @C).
//# programmable --sender A --gas-budget 5000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --gas-payment object(3,0) --inputs 10000 @C
//> 0: test::big::make(Input(0));
//> sui::coin::send_funds<sui::sui::SUI>(Gas, Input(1))

//# create-checkpoint

// A's AB: charged the post-reset net gas (the override was undone, so the
// original smash-target AB(A) pays). The secondary coin was deleted during
// smashing -- that side-effect persists across reset, so the coin's value
// deposit-back is still emitted. Net to A: +deposit_back - net_gas.
//# view-funds sui::balance::Balance<sui::sui::SUI> A

// C's AB should not exist: send_funds was rolled back and override was
// undone, so no funds were ever credited to C and no charge fell on C.
//# view-funds sui::balance::Balance<sui::sui::SUI> C

// Secondary coin should still be deleted (smashing happens before execution
// and is preserved across reset/re-smash).
//# view-object 3,0
