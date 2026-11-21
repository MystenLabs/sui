// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Single gas coin larger than the budget. The PTB sees exactly
// `coin value - gas budget` as the Gas coin. After the surplus is split off,
// the gas coin ends at `gas budget - net gas`.

//# init --addresses test=0x0 --accounts A B

// Two identical coins worth 1 SUI: one for the success case, one for the failure case.
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# view-object 1,0

// Budget 0.5 SUI on a 1 SUI coin: surplus is exactly 500_000_000.
//# programmable --sender A --gas-budget 500000000 --gas-payment object(1,0) --inputs 500000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// Gas coin holds budget minus net gas.
//# view-object 1,0

// The split coin holds the whole surplus.
//# view-object 4,0

// One MIST over the surplus fails; the error reports the exact available balance.
//# programmable --sender A --gas-budget 500000000 --gas-payment object(2,0) --inputs 500000001 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// The failed transaction still pays gas from the coin.
//# view-object 2,0
