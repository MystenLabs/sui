// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test send_funds and redeem_funds from sui::balance

//# init --protocol-version 108 --addresses test=0x0 --accounts A B C --enable-accumulators --enable-address-balance-gas-payments

// Send 1000000000 from A to B
//# programmable --sender A --inputs 1000000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> B

//# view-object 0,1

// Use address balance as gas
//# transfer-object --recipient A --sender B 0,1 --gas-budget 1000000000 --address-balance-gas

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> B
