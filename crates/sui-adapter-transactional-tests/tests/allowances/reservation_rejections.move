// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Reservation-shaped rejections at transaction input validation: over the
// funder's actual balance, a wrong declared funder, a non-Allowance object
// (shared or owned) declared as the allowance, and an allowance that is not
// among the tx inputs. All are free rejections; no allowance state changes.
// The last task pins the flip side: amounts are not checked against the
// allowance's limits at signing, so an over-limit spend aborts at execution.

//# init --accounts A B C

//# programmable --sender A --inputs 5000 @A
// Fund A's (the funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender C --inputs 1000 @C
// Fund C, so the wrong-funder case gets past the balance check and reaches
// the funder comparison.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"capped" @B vector[10000u256] vector[] vector[99999999999999]
// A issues an allowance to B: 10000 lifetime cap, more than A's balance.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender A --inputs b"rated" @B vector[] vector[] vector[] 100000 200u256
// A issues a second allowance to B: rate limit only, 200 per window.
//> 0: sui::allowance::fixed_window(Input(5), Input(6));
//> 1: std::option::some<sui::allowance::RateLimit>(Result(0));
//> 2: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(1));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(6000,@A,object(4,0)) mutshared(4,0) immshared(6)
// Within the 10000 cap but over A's 5000 balance: rejected at signing.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(1), Input(0), Input(2));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@C,object(4,0)) mutshared(4,0) immshared(6)
// B declares C as the funder of A's allowance: rejected at signing.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(1), Input(0), Input(2));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@A,object(6)) immshared(6)
// B declares the Clock as the allowance: rejected at signing.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(1), Input(0), Input(1));

//# programmable --sender B --inputs 100 @B
// A plain coin for the next case.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@A,object(9,0)) object(9,0) immshared(6)
// B declares their own coin as the allowance: rejected at signing.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(1), Input(0), Input(2));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@A,object(4,0)) immshared(6)
// The declared allowance is not an input of the tx: rejected at signing.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(1), Input(0), Input(1));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(300,@A,object(5,0)) mutshared(5,0) immshared(6) @B
// 300 against the rate-limited allowance's 200 per window: admitted at
// signing (limits are not read there) and aborts at execution.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# view-object 4,0

//# view-object 5,0
// current_spend on both is untouched by every attempt.
