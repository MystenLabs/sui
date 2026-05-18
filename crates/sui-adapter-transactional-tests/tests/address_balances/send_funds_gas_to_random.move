// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Unsponsored tx, sender A pays with `[AB(A), Coin]`, workload sends the
// ephemeral gas coin's value to a random third party (C). The reservation
// and the secondary coin's value both flow through the smash to C's AB via
// `send_funds`; the override then redirects the gas charge to AB(C).
// Expected:
//   - A's AB: net change is -reservation. The deposit-back Merge from smash
//     (+coin_value) is offset by a Split of -total_smashed at gas
//     finalization, leaving net -reservation.
//   - C's AB: net change is +total_smashed - net_gas. Receives the ephemeral
//     via send_funds (Merge total_smashed) and is charged gas via the
//     override (Split net_gas).
//   - secondary coin: deleted.
// Combined: A sends reservation + coin_value to C through smash+send_funds;
// C nets out paying the gas.

//# init --addresses test=0x0 --accounts A B C --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

// Seed A's address balance.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Create a 1B coin owned by A -- secondary in the smash.
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-funds sui::balance::Balance<sui::sui::SUI> C

// Pay with [AB(500M), Coin(1B)]; send_funds(Gas, @C). C is a random third party.
//# programmable --sender A --gas-budget 500000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --gas-payment object(2,0) --inputs @C
//> sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0))

//# create-checkpoint

// A's AB ends up with just the deposit-back from the deleted secondary coin
// (no gas charge against A).
//# view-funds sui::balance::Balance<sui::sui::SUI> A

// C's AB receives the ephemeral value and is debited gas via the override.
//# view-funds sui::balance::Balance<sui::sui::SUI> C

//# view-object 2,0
