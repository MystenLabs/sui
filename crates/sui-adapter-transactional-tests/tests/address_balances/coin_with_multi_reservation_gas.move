// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `[gas_coin, withdraw, withdraw]`: real Coin as smash target, multiple
// address-balance reservations from the same sender. The two reservations
// share a payment location and are summed before being smashed in.
// Verifies that:
//   - the smash target stays a real Coin (no ephemeral coin path),
//   - a single accumulator event is emitted for the merged reservation,
//   - leftover reservation lands in the gas coin (not refunded to the balance).

//# init --addresses test=0x0 --accounts A B --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

//# view-object 0,0

// Seed A's address balance.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Pay gas with `[gas_coin, withdraw, withdraw]`. The two withdrawals share
// the same address-balance location and are summed during smashing; the
// smash target is the real gas coin (object 0,0).
//# programmable --sender A --gas-payment object(0,0) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --inputs 100000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// Address balance should have decreased by exactly the net gas usage on the
// reserved amount (1_000_000_000 reserved minus what survives in the coin).
//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Inspect the gas coin: leftover reservation stays here, not refunded to A's
// address balance.
//# view-object 0,0
