// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --enable-feature-flags allow_references_in_ptbs

//# programmable --sender A --inputs 1000 @B
// VALID: `&mut Balance<SUI>` from the gas coin, split and transferred
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: sui::balance::split<sui::sui::SUI>(Result(0), Input(0));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(1));

//# view-object 1,0

//# programmable --sender A --inputs @B
// VALID: the gas coin is drained through a reference; the budget refund still lands
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: sui::balance::withdraw_all<sui::sui::SUI>(Result(0));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(0));

//# view-object 0,0

//# view-object 3,0
