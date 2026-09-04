// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Allowance spends in transactions paying gas from an address balance: the
// spender's gas key alongside the funder's allowance key, and the self case
// where the gas reservation and the allowance reservation land on one key.

//# init --accounts A B

//# programmable --sender A --inputs 10000000000 @A
// Fund A's (the funder and later gas payer) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs 10000000000 @B
// Fund B's (the spender and gas payer) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"tob" @B vector[100000u256] vector[] vector[99999999999999]
// A issues an allowance to B: 100000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --address-balance-gas --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(50000,@A,object(4,0)) mutshared(4,0) immshared(6) @B
// B spends A's allowance while paying gas from B's own balance: the gas key
// and the allowance key settle independently.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# programmable --sender A --inputs b"self" @A vector[100000u256] vector[] vector[99999999999999]
// A issues an allowance to themselves: 100000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender A --address-balance-gas --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(30000,@A,object(6,0)) mutshared(6,0) immshared(6) @B
// The gas reservation and the allowance reservation on the same key: A pays
// gas from the balance their own allowance also debits.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# view-object 4,0
// current_spend is 50000.

//# view-object 6,0
// current_spend is 30000.

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
// 10000000000 - 50000 - 30000, minus the self-spend task's gas.

//# view-funds sui::balance::Balance<sui::sui::SUI> B
// 10000000000 + 50000 + 30000, minus B's spend task's gas.
