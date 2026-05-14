// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Move abort with `[Coin, AddressBalance]` gas payment. The abort triggers a
// temporary-store reset and a re-smash before storage is charged. Verifies
// that re-smashing produces consistent results, and that the final
// accumulator event and gas coin charge reflect the post-abort state, not
// the pre-abort attempt.

//# init --addresses test=0x0 --accounts A --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

//# publish
module test::boom;
public fun boom() { abort 7 }

// Seed A's address balance.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Mixed payment: coin smash target + address-balance secondary; call aborts.
//# programmable --sender A --gas-payment object(0,0) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000)
//> test::boom::boom()

//# create-checkpoint

// Address balance after abort: charge against the merged reservation should
// reflect computation cost (no storage cost since writes were dumped on
// abort).
//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-object 0,0
