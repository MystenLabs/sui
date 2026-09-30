// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `coin::send_funds` is the one Move call that may take the gas coin by value.

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, send_funds takes the gas coin by value while borrowed
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0));
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
