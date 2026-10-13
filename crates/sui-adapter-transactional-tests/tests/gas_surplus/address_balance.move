// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Gas paid purely from the address balance. The reservation equals the budget,
// so the ephemeral Gas coin the PTB sees holds exactly zero.

//# init --addresses test=0x0 --accounts A B

// Seed A's address balance with 10 SUI.
//# programmable --sender A --inputs 10000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Surplus is zero: splitting zero succeeds.
//# programmable --sender A --address-balance-gas --gas-budget 500000000 --inputs 0 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-object 4,0

// Address balance dropped by exactly the net gas.
//# view-funds sui::balance::Balance<sui::sui::SUI> A

// One MIST fails: the ephemeral coin has nothing beyond the budget.
//# programmable --sender A --address-balance-gas --gas-budget 500000000 --inputs 1 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
