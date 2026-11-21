// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A single address-balance reservation larger than the budget. The ephemeral
// Gas coin the PTB sees holds exactly `reservation - gas budget`, and the
// unspent part settles back to the address balance.

//# init --addresses test=0x0 --accounts A B

// Seed A's address balance with 10 SUI.
//# programmable --sender A --inputs 10000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// 1 SUI reserved, 0.5 SUI budget: surplus is exactly 500_000_000.
//# programmable --sender A --gas-budget 500000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(1000000000) --inputs 500000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# view-object 4,0

// Address balance dropped by the surplus sent to B plus the net gas.
//# view-funds sui::balance::Balance<sui::sui::SUI> A

// One MIST over the surplus fails.
//# programmable --sender A --gas-budget 500000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(1000000000) --inputs 500000001 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// Only the net gas of the failed transaction is charged.
//# view-funds sui::balance::Balance<sui::sui::SUI> A
