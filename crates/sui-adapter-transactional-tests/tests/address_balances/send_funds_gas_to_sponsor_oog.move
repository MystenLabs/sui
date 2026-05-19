// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Sponsored tx, sponsor's coin pays gas, workload sends Gas to sponsor's
// address balance and creates a large object that blows storage. On OOG,
// reset drops execution writes (including the send_funds transfer) and
// re-smash undoes the override. The gas charge falls back to the original
// smash-target location -- sponsor's Coin -- not the recipient's AB.

//# init --addresses test=0x0 --accounts A B --enable-address-balance-gas-payments --enable-accumulators

//# publish
module test::big;
public struct W has key, store { id: UID, data: vector<u8> }
public fun make(size: u64, ctx: &mut TxContext) {
    let mut data = vector[];
    let mut i = 0;
    while (i < size) { vector::push_back(&mut data, 0u8); i = i + 1 };
    sui::transfer::public_transfer(W { id: object::new(ctx), data }, ctx.sender());
}

// Sponsored tx with tight budget. Workload makes a big W (storage OOG) then
// send_funds(Gas, @B). Sponsor B's coin (object 0,1) is the gas payment.
//# programmable --sender A --sponsor B --gas-budget 5000000 --inputs 10000 @B
//> 0: test::big::make(Input(0));
//> sui::coin::send_funds<sui::sui::SUI>(Gas, Input(1))

//# create-checkpoint

// Sponsor B's gas coin should still exist (transfer rolled back), owned by
// B, with value reduced by the post-reset net gas.
//# view-object 0,1

// B's AB should not have been created (send_funds and override rolled back).
//# view-funds sui::balance::Balance<sui::sui::SUI> B

// A's AB should not exist either (A never had one and was never charged).
//# view-funds sui::balance::Balance<sui::sui::SUI> A
