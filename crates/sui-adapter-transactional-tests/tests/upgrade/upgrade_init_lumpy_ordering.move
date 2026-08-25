// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Dep_V1=0x0 Dep_V2=0x0 Consumer_V0=0x0 Consumer_V1=0x0 --accounts A

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

//# publish --upgradeable --dependencies Dep_V1 --sender A
module Consumer_V0::m {
    use Dep_V1::d;

    public fun ping() { d::ping() }
}

//# stage-package --dependencies Dep_V2
module Consumer_V1::m {
    use Dep_V1::d;

    public fun ping() { d::ping() }
}
module Consumer_V1::init_dep {
    use Dep_V1::d;

    public struct Config has key { id: sui::object::UID, version: u64 }

    fun init(ctx: &mut sui::tx_context::TxContext) {
        sui::transfer::share_object(Config { id: sui::object::new(ctx), version: d::val() })
    }
}

//# programmable --sender A --inputs object(3,1) 0u8 digest(Consumer_V1)
//> 0: Consumer_V0::m::ping();
//> 1: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 2: Upgrade(Consumer_V1, [Dep_V2,sui,std], Consumer_V0, Result(1));
//> sui::package::commit_upgrade(Input(0), Result(2))

//# programmable --sender A --inputs object(3,1) 0u8 digest(Consumer_V1)
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(Consumer_V1, [Dep_V2,sui,std], Consumer_V0, Result(0));
//> 2: Consumer_V0::m::ping();
//> sui::package::commit_upgrade(Input(0), Result(1))
