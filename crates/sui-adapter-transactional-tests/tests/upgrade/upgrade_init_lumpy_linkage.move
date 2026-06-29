// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Upgrade commands only contribute to the transaction-wide LUMPY linkage when the upgraded package
// introduces an `init` function in a new module. 

//# init --addresses Dep_V1=0x0 Dep_V2=0x0 NoInit_V0=0x0 NoInit_V1=0x0 Init_V0=0x0 Init_V1_Dep1=0x0 Init_V1_Dep2=0x0 --accounts A

//# publish --upgradeable --sender A
module Dep_V1::d {
    public fun val(): u64 { 1 }
    public fun ping() {}
}

//# upgrade --package Dep_V1 --upgrade-capability 1,1 --sender A
module Dep_V2::d {
    public fun val(): u64 { 2 }
    public fun ping() {}
}

//# publish --upgradeable --sender A
module NoInit_V0::m {
    public fun ping() {}
}

//# publish --upgradeable --dependencies Dep_V1 --sender A
module Init_V0::m {
    use Dep_V1::d;

    public fun ping() { d::ping() }
}

//# stage-package --dependencies Dep_V1
module NoInit_V1::m {
    use Dep_V1::d;

    public fun ping() { d::ping() }
}

//# stage-package --dependencies Dep_V1
module Init_V1_Dep1::m {
    public fun ping() {}
}
module Init_V1_Dep1::init_dep {
    use Dep_V1::d;

    public struct Config has key { id: sui::object::UID, v: u64 }

    fun init(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
    }
}

//# stage-package --dependencies Dep_V2
module Init_V1_Dep2::m {
    public fun ping() {}
}
module Init_V1_Dep2::init_dep {
    use Dep_V1::d;

    public struct Config has key { id: sui::object::UID, v: u64 }

    fun init(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
    }
}

//# programmable --sender A --inputs object(3,1) 0u8 digest(NoInit_V1)
// The upgrade has no new-module init, so its Dep_V1 linkage does not conflict with the exact call
// to Dep_V2 in the same PTB.
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(NoInit_V1, [Dep_V1,sui,std], NoInit_V0, Result(0));
//> 2: Dep_V2::d::ping();
//> sui::package::commit_upgrade(Input(0), Result(1))

//# programmable --sender A --inputs object(4,1) 0u8 digest(Init_V1_Dep1)
// The upgrade introduces a new module with init, so its Dep_V1 linkage pins Dep exact(1). Calling
// Dep_V2 in the same PTB requires exact(2), which conflicts.
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(Init_V1_Dep1, [Dep_V1,sui,std], Init_V0, Result(0));
//> 2: Dep_V2::d::ping();
//> sui::package::commit_upgrade(Input(0), Result(1))

//# programmable --sender A --inputs object(4,1) 0u8 digest(Init_V1_Dep2)
// When the upgraded package is already in LUMPY, overlapping upgrade linkage is checked for exact
// equality rather than unified. Init_V0::m::ping puts Init_V0 and at_least(Dep_V1) in LUMPY, so the
// upgrade's Dep_V2 linkage conflicts instead of upgrading the at_least constraint.
//> 0: Init_V0::m::ping();
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(Init_V1_Dep2, [Dep_V2,sui,std], Init_V0, Result(1));
//> sui::package::commit_upgrade(Input(0), Result(2))

//# programmable --sender A --inputs object(4,1) 0u8 digest(Init_V1_Dep2)
// If the upgrade's init linkage and the rest of the PTB agree on Dep_V2, the transaction succeeds
// and init observes Dep_V2.
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(Init_V1_Dep2, [Dep_V2,sui,std], Init_V0, Result(0));
//> 2: Dep_V2::d::ping();
//> sui::package::commit_upgrade(Input(0), Result(1))

//# view-object 11,1
