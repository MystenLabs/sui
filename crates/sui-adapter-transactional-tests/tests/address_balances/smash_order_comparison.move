// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Paired comparison of `[Coin, AddressBalance]` vs `[AddressBalance, Coin]`
// with identical coin sizes, reservations, workloads, and pre-state.
// Exposes the smash-order asymmetry:
//   - `[Coin, AB]`: smash-target coin is mutated. Storage cost includes its
//     re-storage plus any new objects created during execution; rebate
//     refunds the coin's pre-existing rebate; net storage gas is high. The
//     reservation lands in the coin's `balance.value`; AB is debited via a
//     Split accumulator event.
//   - `[AB, Coin]`: the secondary coin is deleted. Storage cost only counts
//     newly created objects; the deleted coin's storage rebate refunds in
//     full with no offsetting re-storage; net storage gas is low. The coin's
//     balance value flows back into AB via a Merge accumulator event; the
//     gas charge against AB happens separately.
// Senders A and B start with identical funded address balances and identical
// 1_000_000_000 coins; the workload is the same (split 100 MIST off Gas and
// TransferObjects to a common third party C). Anything differing between
// the two
// `gas summary` / `accumulators_written` lines is attributable to smash
// order alone.

//# init --addresses test=0x0 --accounts A B C --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

// Seed A's and B's address balances identically.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# programmable --sender B --inputs 100000000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Create identical 1_000_000_000 coins, one per sender.
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender B --inputs 1000000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// ===== Before state =====
//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-funds sui::balance::Balance<sui::sui::SUI> B

//# view-object 3,0

//# view-object 4,0

// ===== Case A: smash [Coin, AB] =====
// Coin (object 3,0, value 1_000_000_000) is the smash target.
// Reservation = 500_000_000 from A's address balance.
//# programmable --sender A --gas-budget 500000000 --gas-payment object(3,0) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --inputs 100 @C
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// ===== Case B: smash [AB, Coin] =====
// Reservation = 500_000_000 from B's address balance is the smash target.
// Coin (object 4,0, value 1_000_000_000) is the secondary; will be deleted.
//# programmable --sender B --gas-budget 500000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --gas-payment object(4,0) --inputs 100 @C
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// ===== After state =====
// A: AB balance dropped by exactly 500M (the Split withdraw event). Coin 3,0
// is still alive, mutated to hold (1B + 500M - case_A_gas_cost).
//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-object 3,0

// B: AB balance shows (start - 500M + 1B - case_B_gas_cost) = start + 500M
// minus case_B_gas. Coin 4,0 no longer exists.
//# view-funds sui::balance::Balance<sui::sui::SUI> B

//# view-object 4,0
