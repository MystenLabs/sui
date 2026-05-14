// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Deepest OOG fallback: when even input-object-only storage charging fails
// (after the usual reset + retry), the engine gives up on charging storage
// and charges the user the full gas budget instead.
// Strategy: pre-create several large input objects (W containing a big
// vector<u8>); on the OOG tx, pass them as inputs and TransferObjects them
// to self with a tight budget. Re-mutating each input costs a small
// non-refundable fee on top of the rebate refund. With big-data objects,
// that non-refundable slice adds up enough that even input-only storage
// can't be paid, forcing the fallback. The snapshot shows A's address
// balance dropping by exactly the budget.

//# init --addresses test=0x0 --accounts A --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

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

// Create 5 big W objects (each holding 10_000 bytes of data).
//# run test::big::make --args 10000 --sender A

//# run test::big::make --args 10000 --sender A

//# run test::big::make --args 10000 --sender A

//# run test::big::make --args 10000 --sender A

//# run test::big::make --args 10000 --sender A

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Pre-state versions of the W inputs (each should be version 2 from creation).
//# view-object 3,0 --hide-contents

//# view-object 4,0 --hide-contents

//# view-object 5,0 --hide-contents

//# view-object 6,0 --hide-contents

//# view-object 7,0 --hide-contents

// OOG tx: pay with AB only, tight budget, pass all 5 big W's as inputs
// and TransferObjects them back to A. Each re-mutation incurs
// non-refundable cost; with 5 big inputs and a tight budget, even
// input-only storage charge fails.
//# programmable --sender A --gas-budget 2500000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(10000000) --inputs object(3,0) object(4,0) object(5,0) object(6,0) object(7,0) @A
//> TransferObjects([Input(0), Input(1), Input(2), Input(3), Input(4)], Input(5))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// After the OOG fallback, the engine drops all writes from execution but
// still forces every mutable input to be re-stored (version bump) for
// safety. All five W's should land on the same post-tx version (the tx's
// lamport timestamp), even though the workload's TransferObjects was rolled
// back.
//# view-object 3,0 --hide-contents

//# view-object 4,0 --hide-contents

//# view-object 5,0 --hide-contents

//# view-object 6,0 --hide-contents

//# view-object 7,0 --hide-contents
