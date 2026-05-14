// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `[Coin, Coin, AddressBalance]`: real Coin as smash target plus a second
// Coin secondary plus an address-balance reservation. Three smash
// side-effects fire together:
//   - the second coin is consumed (deleted),
//   - the reservation emits a single Split accumulator event,
//   - the smash target is mutated with the summed value.

//# init --addresses test=0x0 --accounts A B --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

//# view-object 0,0

// Seed A's address balance.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Create a second SUI coin owned by A (will be smashed in as a secondary).
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Pay gas with `[Coin, Coin, AddressBalance]`. Smash target is the gas coin
// (object 0,0). The second coin (object 3,0) gets deleted; the withdrawal
// secondary emits a `Split, 500_000_000` accumulator event.
//# programmable --sender A --gas-payment object(0,0) --gas-payment object(3,0) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --inputs 100000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// A's address balance should have dropped by exactly 500_000_000.
//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Smash target (object 0,0) holds the leftover (start + secondary coin value
// + reservation - net gas cost). Object 3,0 should no longer exist.
//# view-object 0,0
