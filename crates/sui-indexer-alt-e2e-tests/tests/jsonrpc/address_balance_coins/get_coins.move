// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Fund an address balance for B and verify that suix_getCoins returns the address balance coin.

//# init --protocol-version 108 --addresses Test=0x0 --accounts A B --simulator --enable-accumulators --enable-address-balance-gas-payments

// Send 1_000_000_000 from A to B's address balance
//# programmable --sender A --inputs 1000000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

// B should see the address balance coin in getCoins
//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{B}"]
}
