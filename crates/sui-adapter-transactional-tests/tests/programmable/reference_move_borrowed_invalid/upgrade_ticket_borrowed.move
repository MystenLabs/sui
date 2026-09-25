// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 q=0x0 q_2=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id<T>(t: &T): &T { t }

//# publish --upgradeable --sender A
module q::m {
    public fun x(): u64 { 0 }
}

//# stage-package
module q_2::m {
    public fun x(): u64 { 1 }
}

//# programmable --sender A --inputs object(2,1) 0u8 digest(q_2)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, Upgrade consumes a ticket while a reference into it is live
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: test::m::id<sui::package::UpgradeTicket>(Result(0));
//> 2: Upgrade(q_2, [sui, std], q, Result(0));
//> 3: sui::package::commit_upgrade(Input(0), Result(2));
