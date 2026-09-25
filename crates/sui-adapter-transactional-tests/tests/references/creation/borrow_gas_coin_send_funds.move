// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `coin::send_funds` is the one Move call that may take the gas coin by value.

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_mut<T>(_: &mut T) {}
public fun value_at_least(c: &Coin<SUI>, v: u64) { assert!(c.value() >= v, 0) }

//# programmable --sender A --inputs @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, send_funds takes the gas coin by value while borrowed
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0));
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));

//# programmable --sender A --inputs @B
// INVALID: ArgumentWithoutValue at arg 0 of command 1, the gas coin borrowed after it was transferred
//> 0: TransferObjects([Gas], Input(0));
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(1));

//# programmable --sender A --inputs @B 1
// VALID: the reference dies, then the gas coin is sent
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: test::m::value_at_least(Result(0), Input(1));
//> 2: sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> B
