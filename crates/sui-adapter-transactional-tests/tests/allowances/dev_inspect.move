// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Allowance spends under dev-inspect, the one path that skips
// `check_allowance_inputs`: a valid spend simulates cleanly without
// settling, and a mis-declared funder gets past input validation only to hit
// `consume`'s own funder check.

//# init --accounts A B

//# programmable --sender A --inputs 5000 @A
// Fund A's (the funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs 1000 @B
// Fund B, so the mis-declared funder below has funds to reserve against.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"inspect" @B vector[1000u256] vector[] vector[99999999999999]
// A issues an allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --dev-inspect --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(400,@A,object(4,0)) mutshared(4,0) immshared(6) @B
// A valid spend under dev-inspect.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# programmable --sender B --dev-inspect --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(400,@B,object(4,0)) mutshared(4,0) immshared(6) @B
// B declares themselves as the funder of A's allowance: dev-inspect skips the
// sign-time funder check, and `consume` aborts on it instead.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# create-checkpoint

//# view-object 1,0
// A's balance is untouched by the simulations: 5000.

//# view-object 4,0
// current_spend is 0.
