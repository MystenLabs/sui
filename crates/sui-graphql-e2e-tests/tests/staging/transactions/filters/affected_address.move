// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B C --simulator

// This test involves address A being affected by transactions in various ways:
//
// 1. A splits off a coin from its gas and sends it to B.
//
// 2. A sponsors a transaction where B splits off the a coin from the gas coin
//    and sends it to C.
//
// 3. (A splits off a coin to use as gas in future transactions where the gas
//    coin will be consumed).
//
// 4. (A splits off a coin to use as gas in future transactions where the gas
//    coin will be consumed).
//
// 5. A sends its gas coin to B.
//
// 6. A sponsors a transaction where B sends the gas coin to C.
//
// Then we run a number of GraphQL queries to see whether various addresses are
// considered the sender, recipient or affected by a transaction.

//# programmable --sender A --inputs 1000000 @B
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sponsor A --sender B --inputs 2000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 3000000 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 4000000 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @B --gas-payment 3,0 --gas-budget 3000000
//> TransferObjects([Gas], Input(0))

//# programmable --sponsor A --sender B --inputs @C --gas-payment 4,0 --gas-budget 4000000
//> TransferObjects([Gas], Input(0))

//# create-checkpoint
