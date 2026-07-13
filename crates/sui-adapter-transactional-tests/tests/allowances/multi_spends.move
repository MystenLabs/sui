// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Multiple spends from one allowance in one PTB. Pins: all-or-nothing
// settlement (an abort on the last spend rolls back the earlier ones), the
// sign-vs-execute split for the rate window (signing counts the full window
// amount, execution enforces the remaining), window tumbling through the real
// clock, and same-tx reservation aggregation at signing.

//# init --accounts A B --simulator

//# programmable --sender A --inputs 5000 @A
// Fund A's (the funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"multi" @B vector[10000u256] vector[] vector[] vector[100000] vector[100u256]
// A issues an allowance to B: 10000 lifetime cap, 100 per 100s window.
//> 0: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Input(5), Input(6));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(30,@A,object(3,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(30,@A,object(3,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(30,@A,object(3,0)) mutshared(3,0) immshared(6) @B
// Three spends of 30 in one PTB, all within the window: all settle.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(3), Input(0), Input(4));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(5));
//> 2: sui::allowance::spend_balance<sui::sui::SUI>(Input(3), Input(1), Input(4));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(5));
//> 4: sui::allowance::spend_balance<sui::sui::SUI>(Input(3), Input(2), Input(4));
//> 5: sui::balance::send_funds<sui::sui::SUI>(Result(4), Input(5));

//# view-object 3,0

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(5,@A,object(3,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(10,@A,object(3,0)) mutshared(3,0) immshared(6) @B
// 90 of the 100 window is used at signing, which passes on the full window
// amount since the window may reset before execution. Execution settles the
// first spend, then aborts on the second, rolling back the whole transaction.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(2), Input(0), Input(3));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(4));
//> 2: sui::allowance::spend_balance<sui::sui::SUI>(Input(2), Input(1), Input(3));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(4));

//# view-object 3,0

//# advance-clock --duration-ns 100000000000

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(5,@A,object(3,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(10,@A,object(3,0)) mutshared(3,0) immshared(6) @B
// The identical transaction after the window tumbles: both settle.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(2), Input(0), Input(3));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(4));
//> 2: sui::allowance::spend_balance<sui::sui::SUI>(Input(2), Input(1), Input(3));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(4));

//# view-object 3,0

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(60,@A,object(3,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(60,@A,object(3,0)) mutshared(3,0) immshared(6) @B
// Two 60s aggregate to 120 against the 100 window: rejected at signing.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(2), Input(0), Input(3));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(4));
//> 2: sui::allowance::spend_balance<sui::sui::SUI>(Input(2), Input(1), Input(3));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(4));

//# view-object 3,0
