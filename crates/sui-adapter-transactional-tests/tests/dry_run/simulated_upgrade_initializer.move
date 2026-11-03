// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A dry-run upgrade creates a package and a shared initializer object that must remain
// unnumbered. Its initializer raises a consumer's dependency
// to v2 without consuming the upgrade capability, which a committed upgrade reuses.

//# init --addresses DepV1=0x0 DepV2=0x0 Consumer=0x0 U1=0x0 U2=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m {
    public fun check(expected: u64) { assert!(expected == 1, 0) }

    public fun version(): u64 { 1 }
}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m {
    public fun check(expected: u64) { assert!(expected == 2, 0) }

    public fun version(): u64 { 2 }
}

//# publish --dependencies DepV1 --sender A
module Consumer::m {
    public fun check(expected: u64) { DepV1::m::check(expected) }
}

//# publish --upgradeable --dependencies DepV1 --sender A
module U1::m {
    public fun ping() { DepV1::m::check(1) }
}

//# stage-package --dependencies DepV2
module U2::m {
    public fun ping() { DepV1::m::check(1) }
}
module U2::new_init {
    public struct Config has key { id: UID, version: u64 }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Config { id: object::new(ctx), version: DepV1::m::version() })
    }
}

//# programmable --sender A --inputs object(4,1) 0u8 digest(U2) 2u64 --dry-run
// U is absent from ordinary calls and inputs, so init can raise the consumer to v2.
//> Consumer::m::check(Input(3));
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(U2, [DepV2,sui,std], U1, Result(1));
//> sui::package::commit_upgrade(Input(0), Result(2));

//# programmable --sender A --inputs object(4,1) 0u8 digest(U2) 2u64
// The dry-run did not consume the capability; commit with matching dependency versions.
//> Consumer::m::check(Input(3));
//> DepV2::m::check(Input(3));
//> 2: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 3: Upgrade(U2, [DepV2,sui,std], U1, Result(2));
//> sui::package::commit_upgrade(Input(0), Result(3));

//# view-object 7,1
// The committed initializer observed version 2.
