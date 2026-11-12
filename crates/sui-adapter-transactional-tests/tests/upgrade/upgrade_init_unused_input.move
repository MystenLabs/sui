// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// An unused object whose type belongs to an upgraded package makes that package participate
// in runtime linkage. Tests strict upgrade-init dependency equality, ordinary-command ordering,
// metadata-error precedence, and successful initialization at v2 when ordinary contributions
// already select the required dependency version.

//# init --addresses DepV1=0x0 DepV2=0x0 Consumer=0x0 U1=0x0 U2=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m {
    public fun version(): u64 { 1 }

    public fun ping() {}
}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m {
    public fun version(): u64 { 2 }

    public fun ping() {}
}

//# publish --dependencies DepV1 --sender A
module Consumer::m {
    public fun ping() { DepV1::m::ping() }
}

//# publish --upgradeable --dependencies DepV1 --sender A
module U1::m {
    public struct Obj has key, store { id: UID }

    public fun make(ctx: &mut TxContext) {
        transfer::transfer(Obj { id: object::new(ctx) }, ctx.sender())
    }
}

//# run U1::m::make --sender A

//# stage-package --dependencies DepV2
module U2::m {
    public struct Obj has key, store { id: UID }

    public fun make(ctx: &mut TxContext) {
        transfer::transfer(Obj { id: object::new(ctx) }, ctx.sender())
    }
}
module U2::new_init {
    public struct Config has key { id: UID, version: u64 }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Config { id: object::new(ctx), version: DepV1::m::version() })
    }
}

//# programmable --sender A --inputs object(4,1) 0u8 digest(U2) object(5,0)
// The unused U object makes U participate, so init cannot raise D's existing v1 selection.
//> Consumer::m::ping();
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(U2, [DepV2,sui,std], U1, Result(1));
//> sui::package::commit_upgrade(Input(0), Result(2));

//# programmable --sender A --inputs object(4,1) 0u8 digest(U2) object(5,0)
// The same check applies even when the ordinary dependency contribution follows the upgrade.
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(U2, [DepV2,sui,std], U1, Result(0));
//> Consumer::m::ping();
//> sui::package::commit_upgrade(Input(0), Result(1));

//# programmable --sender A --inputs object(4,1) 0u8 digest(U2) object(5,0)
// Ordinary preparation failures precede deferred upgrade-init conflicts.
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(U2, [DepV2,sui,std], U1, Result(0));
//> Consumer::m::missing();
//> sui::package::commit_upgrade(Input(0), Result(1));

//# programmable --sender A --inputs object(4,1) 0u8 digest(U2) object(5,0)
// Selecting v2 independently agrees with init, even with the unused U input.
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(U2, [DepV2,sui,std], U1, Result(0));
//> DepV2::m::ping();
//> sui::package::commit_upgrade(Input(0), Result(1));

//# view-object 10,1
