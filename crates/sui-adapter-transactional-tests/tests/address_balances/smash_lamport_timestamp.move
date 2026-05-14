// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Lamport-timestamp rule on `[AddressBalance, Coin]` smash: mutable inputs
// end up at max(input versions) + 1 across the full input set, including
// the secondary Coin (which is deleted during smashing).
// Setup:
//   - three W mutable inputs created early at low versions (call them K),
//   - one Coin X created and then bumped (via being the gas object in
//     several minimal txs) to a high version C >> K,
//   - final tx smashes `[withdraw, X]` and force-mutates W1, W2, W3.
// Expected: post-tx versions of W1, W2, W3 all equal C + 1 (not K + 1),
// confirming X's pre-tx version is folded into the tx's lamport timestamp
// even though X itself is deleted.

//# init --addresses test=0x0 --accounts A --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

//# publish
module test::obj;
public struct W has key, store { id: UID }
public fun make(ctx: &mut TxContext) {
    sui::transfer::public_transfer(W { id: object::new(ctx) }, ctx.sender());
}

// Seed A's AB so we have funds for the final tx.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Create three small W mutable inputs early so they end up at low versions.
//# run test::obj::make --sender A

//# run test::obj::make --sender A

//# run test::obj::make --sender A

// Create Coin X (split off from A's default gas coin).
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// Bump X's version by using it as the gas object in three minimal txs.
//# programmable --sender A --gas-budget 5000000 --gas-payment object(6,0) --inputs 1 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --gas-budget 5000000 --gas-payment object(6,0) --inputs 1 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --gas-budget 5000000 --gas-payment object(6,0) --inputs 1 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// Pre-state: W1, W2, W3 are at low versions; X is at a higher version due to
// the bumping above.
//# view-object 3,0 --hide-contents

//# view-object 4,0 --hide-contents

//# view-object 5,0 --hide-contents

//# view-object 6,0 --hide-contents

// Final tx: smash `[withdraw, X]` with W1, W2, W3 as mutable inputs.
// X is the secondary and gets deleted during smashing. Lamport timestamp
// for this tx = max(W*, X, AB accumulator) + 1 = X.version + 1 (since X is
// the highest).
//# programmable --sender A --gas-budget 5000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(5000000) --gas-payment object(6,0) --inputs object(3,0) object(4,0) object(5,0) @A
//> TransferObjects([Input(0), Input(1), Input(2)], Input(3))

//# create-checkpoint

// Post-state: W1, W2, W3 should all be at the same post-tx version, which
// equals (X's pre-tx version) + 1 -- not (W's pre-tx version + 1).
//# view-object 3,0 --hide-contents

//# view-object 4,0 --hide-contents

//# view-object 5,0 --hide-contents
