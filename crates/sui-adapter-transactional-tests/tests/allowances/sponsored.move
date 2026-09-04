// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Allowance spends inside sponsored transactions: a third-party sponsor pays
// gas for the spender, and a funder who is also the tx sponsor.

//# init --accounts A B S

//# programmable --sender A --inputs 5000 @A
// Fund A's (the funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"sponsored" @B vector[1000u256] vector[] vector[99999999999999]
// A issues an allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --sponsor S --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(400,@A,object(3,0)) mutshared(3,0) immshared(6) @B
// B spends A's allowance while S pays the gas.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# programmable --sender S --inputs 3000 @S
// Fund S's (the sponsoring funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender S --inputs b"self-sponsored" @B vector[1000u256] vector[] vector[99999999999999]
// S issues an allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --sponsor S --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(500,@S,object(7,0)) mutshared(7,0) immshared(6) @B
// B spends S's allowance in a tx S sponsors: the funder and the gas owner
// are the same address.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# view-object 3,0
// current_spend is 400.

//# view-object 7,0
// current_spend is 500.

//# create-checkpoint

//# view-object 1,0
// A's settled balance: 5000 - 400 = 4600.

//# view-object 5,0
// S's settled balance: 3000 - 500 = 2500.

//# view-object 4,0
// B's settled balance: 400 + 500 = 900.
