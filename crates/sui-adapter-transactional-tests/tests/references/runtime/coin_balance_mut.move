// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs object(1,0) 300 @B
// VALID: split 300 out through the balance reference into a new coin
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Input(0));
//> 1: sui::balance::split<sui::sui::SUI>(Result(0), Input(1));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(2));

//# view-object 1,0

//# view-object 2,0

//# programmable --sender A --inputs object(1,0) 200
// VALID: join through the balance reference
//> 0: SplitCoins(Gas, [Input(1)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::coin::balance_mut<sui::sui::SUI>(Input(0));
//> 3: sui::balance::join<sui::sui::SUI>(Result(2), Result(1));

//# view-object 1,0
