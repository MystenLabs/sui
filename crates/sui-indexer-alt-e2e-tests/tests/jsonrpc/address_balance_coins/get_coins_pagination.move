// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test pagination of suix_getCoins when address balance coins are mixed with real coins.

//# init --protocol-version 108 --addresses Test=0x0 --accounts A B --simulator --enable-accumulators --enable-address-balance-gas-payments

//# programmable --sender A --inputs 500 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 300 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{B}", null, null, 2]
}

//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{B}"]
}
