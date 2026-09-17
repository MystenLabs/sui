// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A legacy coin reservation input that no command of the PTB uses. The
// conversion to a `Coin` is injected ahead of the original command and the
// send-back of the unused coin is injected after it.

//# init --addresses test=0x0 --accounts A B

// Seed A's address balance.
//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs coin_reservation<sui::balance::Balance<sui::sui::SUI>>(500000000)
//> sui::coin::value<sui::sui::SUI>(Gas)

//# create-checkpoint

// The reservation is withdrawn and sent back, so A's balance is unchanged.
//# view-funds sui::balance::Balance<sui::sui::SUI> A
