// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// An allowance spend and the sender's own withdrawal in one PTB: two
// reservation keys (funder and sender), each settled against its own
// address balance.

//# init --accounts A B

//# programmable --sender A --inputs 5000 @A
// Fund A's (the funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs 1000 @B
// Fund B's (the spender) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"mixed" @B vector[1000u256] vector[] vector[99999999999999]
// A issues an allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(400,@A,object(4,0)) withdraw<sui::balance::Balance<sui::sui::SUI>>(300) mutshared(4,0) immshared(6) @A @B
// B spends 400 of A's allowance to themselves and 300 of their own balance
// to A. Settlement nets per address: A -100, B +100.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(2), Input(0), Input(3));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(5));
//> 2: sui::balance::redeem_funds<sui::sui::SUI>(Input(1));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(4));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(200,@A,object(4,0)) mutshared(4,0) immshared(6) @A
// B spends 200 and sends all of it back to A: the flow nets to zero at both
// addresses, yet the allowance is still charged the gross amount.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# view-object 4,0
// current_spend is 600: the gross 400 + 200; the sender-sourced withdrawal
// and the net-zero settlement never touched it.

//# create-checkpoint

//# view-object 1,0
// A's settled balance: 5000 - 400 + 300 = 4900; the round trip nets out.

//# view-object 2,0
// B's settled balance: 1000 - 300 + 400 = 1100.
