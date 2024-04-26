// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses V0=0x0 V1=0x0 V2=0x0 V3=0x0 --accounts A

//# publish --upgradeable --sender A
module V0::base_module {
    public struct Object has key, store {
        id: UID,
        field0: u64,
        field1: u64,
    }

    public fun public_fun(): u64 { 0 }
}
module V0::a {
    fun call_friend(): u64 { V0::base_module::public_fun() }
}
module V0::b {
    public fun public_fun(): u64 { 0 }
}
module V0::other_module {
    public struct Y { }
    fun public_fun(): u64 { 0 }
}

// other_module::Y is missing in V1
//# upgrade --package V0 --upgrade-capability 1,1 --sender A
module V1::base_module {
    public struct Object has key, store {
        id: UID,
        field0: u64,
        field1: u64,
    }

    public fun public_fun(): u64 { 0 }
}
module V1::a {
    fun call_friend(): u64 { V0::base_module::public_fun() }
}
module V1::b {
    public fun public_fun(): u64 { 0 }
}
module V1::other_module {
    fun public_fun(): u64 { 0 }
}

// other_module missing in V1
//# upgrade --package V0 --upgrade-capability 1,1 --sender A
module V1::base_module {
    public struct Object has key, store {
        id: UID,
        field0: u64,
        field1: u64,
    }

    public fun public_fun(): u64 { 0 }
}
module V1::a {
    fun call_friend(): u64 { V0::base_module::public_fun() }
}
module V1::b {
    public fun public_fun(): u64 { 0 }
}

// `b` missing in V1
//# upgrade --package V0 --upgrade-capability 1,1 --sender A
module V1::base_module {
    public struct Object has key, store {
        id: UID,
        field0: u64,
        field1: u64,
    }

    public fun public_fun(): u64 { 0 }
}
module V1::a {
    fun call_friend(): u64 { V0::base_module::public_fun() }
}
module V1::other_module {
    public struct Y { }
    fun public_fun(): u64 { 0 }
}

// `a` missing in V1
//# upgrade --package V0 --upgrade-capability 1,1 --sender A
module V0::base_module {
    public struct Object has key, store {
        id: UID,
        field0: u64,
        field1: u64,
    }

    public fun public_fun(): u64 { 0 }
}
module V0::b {
    public fun public_fun(): u64 { 0 }
}
module V0::other_module {
    public struct Y { }
    fun public_fun(): u64 { 0 }
}
