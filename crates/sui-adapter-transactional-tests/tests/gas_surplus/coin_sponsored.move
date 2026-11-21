// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Sponsored transaction paying with the sponsor's coin. The sender's PTB sees
// `coin value - gas budget` as the Gas coin, exactly as the owner would.

//# init --addresses test=0x0 --accounts A B C

// Two identical sponsor-owned coins worth 1 SUI.
//# programmable --sender B --inputs 1000000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender B --inputs 1000000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// Sender A spends the surplus of sponsor B's coin: 1 SUI - 0.5 SUI budget.
//# programmable --sender A --sponsor B --gas-budget 500000000 --gas-payment object(1,0) --inputs 500000000 @C
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// Sponsor's gas coin holds budget minus net gas.
//# view-object 1,0

//# view-object 3,0

// One MIST over the surplus fails.
//# programmable --sender A --sponsor B --gas-budget 500000000 --gas-payment object(2,0) --inputs 500000001 @C
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# view-object 2,0
