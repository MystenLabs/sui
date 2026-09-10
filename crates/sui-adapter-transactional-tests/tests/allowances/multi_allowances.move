// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Spending through several allowances in one PTB: two with the same funder
// (reservations coalesce at one key), two with different funders (two keys),
// and a withdrawal redeemed against the wrong allowance object, which only
// execution can catch.

//# init --accounts A B C

//# programmable --sender A --inputs 5000 @A
// Fund A's (the first funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender C --inputs 2000 @C
// Fund C's (the second funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"first" @B vector[1000u256] vector[] vector[99999999999999]
// A issues a first allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender A --inputs b"second" @B vector[1000u256] vector[] vector[99999999999999]
// A issues a second allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender C --inputs b"third" @B vector[1000u256] vector[] vector[99999999999999]
// C issues an allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(300,@A,object(4,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(200,@A,object(5,0)) mutshared(4,0) mutshared(5,0) immshared(6) @B
// B spends A's two allowances in one PTB: the reservations coalesce at A's
// key and settle as one Split.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(2), Input(0), Input(4));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(5));
//> 2: sui::allowance::balance_spend<sui::sui::SUI>(Input(3), Input(1), Input(4));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(5));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@A,object(4,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(400,@C,object(6,0)) mutshared(4,0) mutshared(6,0) immshared(6) @B
// B spends A's and C's allowances in one PTB: two keys, two Splits.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(2), Input(0), Input(4));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(5));
//> 2: sui::allowance::balance_spend<sui::sui::SUI>(Input(3), Input(1), Input(4));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(5));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(50,@A,object(4,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(50,@A,object(5,0)) mutshared(4,0) mutshared(5,0) immshared(6) @B
// Both declarations are valid, but the withdrawal minted for the first
// allowance is redeemed against the second: signing admits it, execution
// aborts in `consume`, and nothing is debited.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(3), Input(0), Input(4));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(5));

//# view-object 4,0
// current_spend is 400: 300 + 100, untouched by the failed crossing.

//# view-object 5,0
// current_spend is 200.

//# view-object 6,0
// current_spend is 400.

//# create-checkpoint

//# view-object 1,0
// A's settled balance: 5000 - 500 - 100 = 4400.

//# view-object 2,0
// C's settled balance: 2000 - 400 = 1600.

//# view-object 7,0
// B's settled balance: 500 + 500 = 1000.
