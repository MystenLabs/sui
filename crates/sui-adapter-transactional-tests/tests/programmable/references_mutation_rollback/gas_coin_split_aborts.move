// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun fail() { abort 42 }

//# view-object 0,0

//# programmable --sender A --inputs 100 @A
// ABORTS: fail aborts with code 42 at command 4 after splitting 100 through a gas balance reference
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: sui::balance::split<sui::sui::SUI>(Result(0), Input(0));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(1));
//> 4: test::m::fail();

//# view-object 0,0
