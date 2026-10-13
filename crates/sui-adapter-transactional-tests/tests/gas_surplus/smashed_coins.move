// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Two gas coins smashed together. The PTB sees `sum of coin values - gas budget`
// as the Gas coin; the smash target keeps `gas budget - net gas` at the end and
// the secondary coin is deleted.

//# init --addresses test=0x0 --accounts A B

// Two pairs of coins worth 0.6 SUI and 0.4 SUI.
//# programmable --sender A --inputs 600000000 400000000 @A
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(2))

//# programmable --sender A --inputs 600000000 400000000 @A
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(2))

//# view-object 1,0

//# view-object 1,1

// Budget 0.5 SUI on 1 SUI smashed: surplus is exactly 500_000_000.
//# programmable --sender A --gas-budget 500000000 --gas-payment object(1,0) --gas-payment object(1,1) --inputs 500000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// Smash target holds budget minus net gas.
//# view-object 1,0

//# view-object 5,0

// One MIST over the surplus fails.
//# programmable --sender A --gas-budget 500000000 --gas-payment object(2,0) --gas-payment object(2,1) --inputs 500000001 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# view-object 2,0
