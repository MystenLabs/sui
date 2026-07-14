// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// One allowance declared twice in the same transaction with different
// funders. The first declaration resolves and caches the allowance; the
// second must still get its own funder check, so the whole tx is rejected.

//# init --accounts A B C

//# programmable --sender A --inputs 5000 @A
// Fund A's (the funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender C --inputs 1000 @C
// Fund C, so the mismatched declaration gets past the balance check and
// reaches the funder comparison.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"dup" @B vector[10000u256] vector[] vector[99999999999999] vector[] vector[]
// A issues an allowance to B: 10000 lifetime cap.
//> 0: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Input(5), Input(6));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@A,object(4,0)) allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@C,object(4,0)) mutshared(4,0) immshared(6)
// Correct funder first, wrong funder second: rejected at signing.
//> 0: sui::allowance::spend_balance<sui::sui::SUI>(Input(2), Input(0), Input(3));
//> 1: sui::allowance::spend_balance<sui::sui::SUI>(Input(2), Input(1), Input(3));

//# view-object 4,0
// current_spend is untouched.
