// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 q=0x0 q_2=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_mut<T>(_: &mut T) {}

//# publish --upgradeable --sender A
module q::m {
    public fun x(): u64 { 0 }
}

//# stage-package
module q_2::m {
    public fun x(): u64 { 1 }
}

//# programmable --sender A --inputs object(2,1) 0u8 digest(q_2)
// INVALID: InvalidReferenceArgument at arg 0 of command 1, the cap `&mut` while a reference into it is live
//> 0: test::m::id_mut<sui::package::UpgradeCap>(Input(0));
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: test::m::use_mut<sui::package::UpgradeCap>(Result(0));
//> 3: Upgrade(q_2, [sui, std], q, Result(1));
//> 4: sui::package::commit_upgrade(Input(0), Result(3));
