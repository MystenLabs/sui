// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `[AddressBalance, Coin, Coin]`: address-balance withdrawal as smash target
// with two Coin secondaries.
//   - both coins are consumed (deleted),
//   - surplus (total smashed minus reservation) is deposited back into the
//     address balance via a single Merge accumulator event,
//   - the gas charge against the address balance happens at final charging
//     time via a separate Split event.

//# init --addresses test=0x0 --accounts A B --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

//# view-object 0,0

// Seed A's address balance.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Create two extra SUI coins to be smashed into the withdrawal target.
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Pay with `[AddressBalance, Coin, Coin]`. Smash target is the reservation;
// both coins are deleted. total_smashed = 500_000_000 + 1_000_000_000 +
// 1_000_000_000 = 2_500_000_000; deposit-back = 2_000_000_000.
//# programmable --sender A --gas-budget 500000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --gas-payment object(3,0) --gas-payment object(4,0) --inputs 100000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// A's address balance should reflect: starting balance + 2B deposit-back -
// net gas charge. The deposit-back event and the gas charge event should be
// visible as separate accumulator entries in the snapshot.
//# view-funds sui::balance::Balance<sui::sui::SUI> A
