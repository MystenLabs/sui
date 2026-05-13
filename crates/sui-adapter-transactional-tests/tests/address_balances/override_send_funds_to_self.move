// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Esoteric override edge case: `[AddressBalance(A), Coin]` smash target =
// AB, secondary Coin gets deleted; then `send_funds(Gas, @A)` transfers the
// ephemeral gas coin's value back into A's address balance. At gas
// finalization, the gas-charge location is overridden to the recipient's
// address balance -- which is the SAME address as the original payer, so
// the override is logically a no-op. This test pins down that the gas
// accounting still settles correctly.
// Compared to the existing pure-address-balance send_funds-to-self test,
// this adds a secondary Coin to the smash so the secondary-coin deletion
// and Merge deposit-back paths run too.

//# init --addresses test=0x0 --accounts A --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

// Seed A's address balance.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Create a 1B coin owned by A - this will be the secondary in the smash.
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-object 2,0

// Pay with [AB(500M), Coin(1B)]; send_funds(Gas, @A) - recipient is the
// original AB owner. The override target equals the original gas charge
// location.
//# programmable --sender A --gas-budget 500000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --gas-payment object(2,0) --inputs @A
//> sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0))

//# create-checkpoint

// After: A's AB should reflect all accumulator events - deposit-back from
// smashing the secondary coin (Merge: coin value), the send_funds transfer
// of the ephemeral coin to A's AB (Merge: full smashed value), the override-
// driven debit (Split: full smashed value), and the final gas charge.
//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-object 2,0
