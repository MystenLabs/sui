// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test gasless_send_funds and redeem_funds from sui::balance (mirrors withdraw_and_send_account)

//# init --addresses test=0x0 --accounts A B --enable-free-tier --enable-accumulators --simulator

// Send 1000 from A to B using gasless_send_funds
//# programmable --sender A --inputs 1000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::gasless_send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

// B withdraws 500 and sends to A using gasless_send_funds
//# programmable --sender B --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(500) @A
//> 0: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 1: sui::balance::gasless_send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

// B withdraws 500 and sends to self using gasless_send_funds
//# programmable --sender B --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(500) @B
//> 0: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 1: sui::balance::gasless_send_funds<sui::sui::SUI>(Result(0), Input(1));
