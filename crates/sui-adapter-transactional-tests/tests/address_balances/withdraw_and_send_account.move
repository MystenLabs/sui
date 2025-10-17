// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test send_funds and redeem_funds from sui::balance

//# init --addresses test=0x0 --accounts A B --enable-accumulators --simulator

// Send 1000 from A to B
//# programmable --sender A --inputs 1000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

// B withdraws 500 and send to A
//# programmable --sender B --inputs withdraw(500,sui::balance::Balance<sui::sui::SUI>) @A
//> 0: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

// B withdraws 500 and send to self
//# programmable --sender B --inputs withdraw(500,sui::balance::Balance<sui::sui::SUI>) @B
//> 0: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(1));
