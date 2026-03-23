// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests gas payment using withdrawals (drawing from address balance) combined
// with real coin objects.

//# init --addresses test=0x0 --accounts A B --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

//# view-object 0,0

//# programmable --sender A --inputs 100000000000 @A
// send funds to A's address balance so we can use coin reservations
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# programmable --sender A --gas-payment object(0,0) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --inputs 100000 @B
// split from gas with object first then withdrawal
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1000000000 @A
// create a coin to use for the reservation-first test (needs enough for gas budget)
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --gas-budget 500000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --gas-payment object(5,0) --inputs 100000 @B
// split from gas with withdrawal first then object (uses the coin we just created)
// note: this will DELETE the coin being merged into the withdrawal
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 200000 @A
// create a coin to merge into gas
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --gas-payment object(0,0) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --inputs object(7,0)
// merge coin into gas when using withdrawal
//> MergeCoins(Gas, [Input(0)])

//# view-object 0,0

//# programmable --sender A --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(6000000000) --inputs @B
// transfer gas coin to B while paying with withdrawal only
//> TransferObjects([Gas], Input(0))

//# view-object 10,0

//# programmable --sender A --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(3000000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(3000000000) --inputs 1000 @B
// use multiple withdrawals
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# view-object 12,0

//# programmable --sender A --inputs 200000 @A
// create a coin to merge into gas (for pure withdrawal test)
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 200000 @A
// create a coin to merge into gas (for pure address balance test)
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(5000000000) --inputs object(14,0)
// merge coin into gas when paying with pure withdrawal
//> MergeCoins(Gas, [Input(0)])

//# programmable --sender A --address-balance-gas --inputs object(15,0)
// merge coin into gas when paying with pure address balance
//> MergeCoins(Gas, [Input(0)])
