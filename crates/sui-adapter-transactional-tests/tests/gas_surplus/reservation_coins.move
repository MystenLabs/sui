// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Address-balance reservation as smash target plus a gas coin. The PTB sees
// `reservation + coin value - gas budget` as the ephemeral Gas coin. The coin
// is deleted at smash time and its value deposited back to the address balance.

//# init --addresses test=0x0 --accounts A B

// Seed A's address balance with 10 SUI.
//# programmable --sender A --inputs 10000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Two identical coins worth 0.6 SUI.
//# programmable --sender A --inputs 600000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 600000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// 0.4 SUI reservation + 0.6 SUI coin - 0.5 SUI budget: surplus is exactly 500_000_000.
//# programmable --sender A --gas-budget 500000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(400000000) --gas-payment object(2,0) --inputs 500000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-object 6,0

// Address balance: + 0.6 SUI deposited back - 0.5 SUI surplus sent to B - net gas.
//# view-funds sui::balance::Balance<sui::sui::SUI> A

// One MIST over the surplus fails.
//# programmable --sender A --gas-budget 500000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(400000000) --gas-payment object(3,0) --inputs 500000001 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
