// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun value_at_least(c: &Coin<SUI>, v: u64) { assert!(c.value() >= v, 0) }

//# programmable --sender A --inputs 1000
// VALID: `&Coin<SUI>` from a `&mut` gas reference (freeze)
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: test::m::value_at_least(Result(0), Input(0));
