// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 q=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_mut<T>(_: &mut T) {}

//# publish --upgradeable --sender A
module q::m {
    public fun x(): u64 { 0 }
}

//# programmable --sender A --inputs object(2,1)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, the cap by value while a reference into it is live
//> 0: test::m::id_mut<sui::package::UpgradeCap>(Input(0));
//> 1: sui::package::make_immutable(Input(0));
//> 2: test::m::use_mut<sui::package::UpgradeCap>(Result(0));
