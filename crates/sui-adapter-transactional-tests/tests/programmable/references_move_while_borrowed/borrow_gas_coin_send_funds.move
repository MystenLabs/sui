// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `coin::send_funds` is the one Move call that may take the gas coin by value.

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public fun id_mut<T>(t: &mut T): &mut T { t }

public fun value_at_least(c: &Coin<SUI>, v: u64) { assert!(c.value() >= v, 0) }

//# programmable --sender A --inputs @B 1
// VALID: the reference dies, then the gas coin is sent
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: test::m::value_at_least(Result(0), Input(1));
//> 2: sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> B
