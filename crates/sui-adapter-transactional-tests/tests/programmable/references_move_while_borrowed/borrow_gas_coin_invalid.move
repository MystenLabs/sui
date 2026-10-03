// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }

public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, gas transferred while borrowed
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: TransferObjects([Gas], Input(0));
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
