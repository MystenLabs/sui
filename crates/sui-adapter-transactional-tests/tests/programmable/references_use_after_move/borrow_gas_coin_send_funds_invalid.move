// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `coin::send_funds` is the one Move call that may take the gas coin by value.

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs @B
// INVALID: ArgumentWithoutValue at arg 0 of command 1, the gas coin borrowed after it was transferred
//> 0: TransferObjects([Gas], Input(0));
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(1));
