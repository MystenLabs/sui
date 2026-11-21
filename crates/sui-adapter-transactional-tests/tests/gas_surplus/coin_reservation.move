// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Gas coin as smash target plus an address-balance reservation. The PTB sees
// `coin value + reservation - gas budget` as the Gas coin. The reservation is
// withdrawn in full at smash time, so the leftover lands in the coin.

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

// 0.6 SUI coin + 0.4 SUI reservation - 0.5 SUI budget: surplus is exactly 500_000_000.
//# programmable --sender A --gas-budget 500000000 --gas-payment object(2,0) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(400000000) --inputs 500000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// Gas coin holds budget minus net gas.
//# view-object 2,0

//# view-object 6,0

// Address balance dropped by exactly the reservation.
//# view-funds sui::balance::Balance<sui::sui::SUI> A

// One MIST over the surplus fails.
//# programmable --sender A --gas-budget 500000000 --gas-payment object(3,0) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(400000000) --inputs 500000001 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-object 3,0

//# view-funds sui::balance::Balance<sui::sui::SUI> A
