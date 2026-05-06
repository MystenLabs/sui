// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Benchmark the send_funds command with a SUI withdrawal from A's address balance
// sent to B, with gas also paid from A's address balance.

//# init --addresses test=0x0 --accounts A B C D E --enable-accumulators --enable-address-balance-gas-payments

// Seed A's address balance so it can both fund the withdrawal and pay for gas.
//# programmable --sender A --inputs 20000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

// Benchmark: withdraw 100 from A's address balance and send_funds to B,
// with gas paid from the address balance.
//# bench ptb --sender A --address-balance-gas --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(100) @B
//> 0: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(1));

// Benchmark: withdraw 400 from A's address balance and send it to B, C, D, and E
// with gas paid from the address balance.
//# bench ptb --sender A --address-balance-gas --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(100) withdraw<sui::balance::Balance<sui::sui::SUI>>(100) withdraw<sui::balance::Balance<sui::sui::SUI>>(100) withdraw<sui::balance::Balance<sui::sui::SUI>>(100) @B @C @D @E
//> 0: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 1: sui::balance::redeem_funds<sui::sui::SUI>(Input(1));
//> 2: sui::balance::redeem_funds<sui::sui::SUI>(Input(2));
//> 3: sui::balance::redeem_funds<sui::sui::SUI>(Input(3));
//> 4: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(4));
//> 5: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(5));
//> 6: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(6));
//> 7: sui::balance::send_funds<sui::sui::SUI>(Result(3), Input(7));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-funds sui::balance::Balance<sui::sui::SUI> B
