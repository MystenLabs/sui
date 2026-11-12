// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// An upgrade retaining an initializer in an existing module must neither rerun it nor add
// its declared dependencies to runtime linkage. An aborting retained initializer and an
// ordinary exact-v2 call establish both properties; the capability records a successful upgrade.

//# init --addresses DepV1=0x0 DepV2=0x0 U1=0x0 U2=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m {
    public fun check(expected: u64) { assert!(expected == 1, 0) }
}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m {
    public fun check(expected: u64) { assert!(expected == 2, 0) }
}

//# publish --upgradeable --dependencies DepV1 --sender A
module U1::m {
    fun init(_: &mut TxContext) { DepV1::m::check(1) }

    public fun ping() { DepV1::m::check(1) }
}

//# stage-package --dependencies DepV1
module U2::m {
    // Retaining an initializer must neither rerun it nor pin its declared Dep v1 in runtime linkage.
    fun init(_: &mut TxContext) { abort 99 }

    public fun ping() { DepV1::m::check(1) }
}

//# programmable --sender A --inputs object(3,1) 0u8 digest(U2) 2u64
// There is no new-module init, so an exact v2 call does not conflict with the upgrade's v1 deps.
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(U2, [DepV1,sui,std], U1, Result(0));
//> DepV2::m::check(Input(3));
//> sui::package::commit_upgrade(Input(0), Result(1));

//# view-object 3,1
// The capability records a successful version-2 upgrade despite the retained aborting init.
