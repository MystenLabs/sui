// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Additional end-to-end coverage for upgrade-init Lumpy examples

//# init --addresses Dep_V1=0x0 Dep_V2=0x0 ConsumerV1=0x0 Extra=0x0 Agree_V0=0x0 Agree_V1=0x0 A0=0x0 A1_Dep1=0x0 D0=0x0 D1_Dep1=0x0 D1_Dep2=0x0 Refine_V0=0x0 Refine_V1_Dep2=0x0 --accounts A

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

//# publish --dependencies Dep_V1 --sender A
module ConsumerV1::c {
    use Dep_V1::d;

    public fun consume() { d::ping() }
}

//# publish --sender A
module Extra::e {
    public fun ping() {}
}

//# publish --upgradeable --dependencies Dep_V1 --sender A
module Agree_V0::m {
    use Dep_V1::d;

    public fun ping() { d::ping() }
}

//# publish --upgradeable --sender A
module A0::m {
    public fun ping() {}
}

//# publish --upgradeable --sender A
module D0::m {
    public fun ping() {}
}

//# publish --upgradeable --sender A
module Refine_V0::m {
    public fun ping() {}
}

//# stage-package --dependencies Dep_V1 Extra
module Agree_V1::m {
    public fun ping() {}
}
module Agree_V1::init_dep {
    use Dep_V1::d;

    public struct Config has key { id: sui::object::UID, v: u64 }

    fun init(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
    }
}

//# stage-package --dependencies Dep_V1
module A1_Dep1::m {
    public fun ping() {}
}
module A1_Dep1::init_dep {
    use Dep_V1::d;

    public struct Config has key { id: sui::object::UID, v: u64 }

    fun init(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
    }
}

//# stage-package --dependencies Dep_V1
module D1_Dep1::m {
    public fun ping() {}
}
module D1_Dep1::init_dep {
    use Dep_V1::d;

    public struct Config has key { id: sui::object::UID, v: u64 }

    fun init(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
    }
}

//# stage-package --dependencies Dep_V2
module D1_Dep2::m {
    public fun ping() {}
}
module D1_Dep2::init_dep {
    use Dep_V1::d;

    public struct Config has key { id: sui::object::UID, v: u64 }

    fun init(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
    }
}

//# stage-package --dependencies Dep_V2
module Refine_V1_Dep2::m {
    public fun ping() {}
}
module Refine_V1_Dep2::init_dep {
    use Dep_V1::d;

    public struct Config has key { id: sui::object::UID, v: u64 }

    fun init(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
    }
}

//# programmable --sender A --inputs object(5,1) 0u8 digest(Agree_V1)
// The upgraded package is already in Lumpy and overlapping linkage agrees. The upgrade
// also introduces Extra as an exact dependency that was not already in Lumpy.
//> 0: Agree_V0::m::ping();
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(Agree_V1, [Dep_V1,Extra,sui,std], Agree_V0, Result(1));
//> sui::package::commit_upgrade(Input(0), Result(2))

//# programmable --sender A --inputs object(6,1) 0u8 digest(A1_Dep1) object(7,1) 0u8 digest(D1_Dep2)
// Two different upgrades with new-module init conflict through overlapping dependency
// linkage: A pins Dep_V1, while D pins Dep_V2.
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(A1_Dep1, [Dep_V1,sui,std], A0, Result(0));
//> 2: sui::package::commit_upgrade(Input(0), Result(1));
//> 3: sui::package::authorize_upgrade(Input(3), Input(4), Input(5));
//> 4: Upgrade(D1_Dep2, [Dep_V2,sui,std], D0, Result(3));
//> sui::package::commit_upgrade(Input(3), Result(4))

//# programmable --sender A --inputs object(6,1) 0u8 digest(A1_Dep1) object(7,1) 0u8 digest(D1_Dep1)
// Two different upgrades with new-module init agree on overlapping dependency linkage.
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(A1_Dep1, [Dep_V1,sui,std], A0, Result(0));
//> 2: sui::package::commit_upgrade(Input(0), Result(1));
//> 3: sui::package::authorize_upgrade(Input(3), Input(4), Input(5));
//> 4: Upgrade(D1_Dep1, [Dep_V1,sui,std], D0, Result(3));
//> sui::package::commit_upgrade(Input(3), Result(4))

//# programmable --sender A --inputs object(8,1) 0u8 digest(Refine_V1_Dep2)
// If the upgraded package is not already in Lumpy, upgrade-init exact linkage uses normal
// unification and can refine an existing at_least(Dep_V1) constraint to exact(Dep_V2).
//> 0: ConsumerV1::c::consume();
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(Refine_V1_Dep2, [Dep_V2,sui,std], Refine_V0, Result(1));
//> sui::package::commit_upgrade(Input(0), Result(2))

//# view-object 17,1
