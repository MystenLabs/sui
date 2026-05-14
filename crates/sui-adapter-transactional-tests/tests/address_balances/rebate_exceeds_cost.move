// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Storage rebate exceeds storage cost plus computation, so net gas usage is
// negative (a refund). Smash `[AB, Coin, Coin, Coin]` with a trivial workload
// (one SplitCoins + TransferObjects, creating one new coin):
//   - 3 secondary coins deleted -> ~3 coins' worth of storage rebate
//   - 1 new coin created        -> ~1 coin's worth of storage cost
//   - small computation cost
// With AB as the smash target, the surplus is credited back to A's address
// balance via an accumulator event on top of the coin-value deposit-back
// (3 coins' balance values folded into AB, minus the 100 MIST routed out to
// B as a new coin).

//# init --addresses test=0x0 --accounts A B --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

// Seed A's address balance.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Create three identical 1_000_000_000 coins for A (will be smashed/deleted).
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-object 2,0

//# view-object 3,0

//# view-object 4,0

// Smash [AB(5M), Coin(1B), Coin(1B), Coin(1B)] with trivial workload. AB is
// the smash target so net gas refund + coin-value deposit both land in AB.
//# programmable --sender A --gas-budget 5000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(5000000) --gas-payment object(2,0) --gas-payment object(3,0) --gas-payment object(4,0) --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// After: A's AB should be ~ 100B + 3B (coin values) + |net_gas_refund|.
// The three coins are gone.
//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-object 2,0

//# view-object 3,0

//# view-object 4,0
