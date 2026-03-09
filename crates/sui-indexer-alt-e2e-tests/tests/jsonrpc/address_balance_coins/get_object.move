// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Fund an address balance, get the address balance coin's object ID from getCoins, then call
// sui_getObject with it to verify the synthesized Coin object is returned.
// Note: The dynamic getObject flow (extracting coin ID from getCoins response) is tested in the
// Rust e2e tests. Here we verify getCoins returns the address balance coin correctly.

//# init --protocol-version 108 --addresses Test=0x0 --accounts A B --simulator --enable-accumulators --enable-address-balance-gas-payments

// Send 1_000_000_000 from A to B's address balance
//# programmable --sender A --inputs 1000000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

// Get B's coins — should include the address balance coin
//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{B}"]
}
