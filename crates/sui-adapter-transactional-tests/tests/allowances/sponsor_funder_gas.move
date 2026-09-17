// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The funder is also the gas sponsor, paying gas from their own address
// balance. Two reservation sources -- the gas budget and the allowance
// withdrawal -- coalesce on one key that is neither the sender's nor a
// self-allowance, and the net withdrawal is capped by their sum.

//# init --accounts B S

//# programmable --sender S --inputs 10000000000 @S
// Fund S's (the funder and gas sponsor) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender S --inputs b"sponsor-funder" @B vector[100000u256] vector[] vector[99999999999999]
// S issues an allowance to B: 100000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --sponsor S --address-balance-gas --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(30000,@S,object(3,0)) mutshared(3,0) immshared(6) @B
// B spends S's allowance in a tx S sponsors from S's address balance: the gas
// key and the allowance key are the same (S, Balance<SUI>), and S is neither
// the sender nor the funder of a self-allowance.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# view-object 3,0
// current_spend is 30000: only the allowance-sourced spend, not the gas.

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> S
// 10000000000 - 30000, minus the sponsored task's gas.

//# view-funds sui::balance::Balance<sui::sui::SUI> B
// 30000.
