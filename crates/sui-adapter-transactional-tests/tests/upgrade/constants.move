// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses V0=0x0 V1=0x0 V2=0x0 V3=0x0 V4=0x0 V5=0x0 V6=0x0 --accounts A

//# publish --upgradeable --sender A
module V0::base_module {

    const A: u64 = 0;

    public struct X {
        field0: u64,
        field1: u64,
    }
    public fun public_fun(): u64 { A }
}

//# upgrade --package V0 --upgrade-capability 1,1 --sender A --policy compatible
module V1::base_module {

    const A: u64 = 1;

    public struct X {
        field0: u64,
        field1: u64,
    }
    public fun public_fun(): u64 { A }
}

//# upgrade --package V1 --upgrade-capability 1,1 --sender A --policy additive
module V2::base_module {

    const Y: u64 = 1;

    public struct X {
        field0: u64,
        field1: u64,
    }
    public fun public_fun(): u64 { Y }
}

// Value of the constant has changed -- this is incompatible in additive and dep_only policies

//# upgrade --package V2 --upgrade-capability 1,1 --sender A --policy additive
module V3::base_module {

    const Y: u64 = 0;

    public struct X {
        field0: u64,
        field1: u64,
    }
    public fun public_fun(): u64 { Y }
}

//# upgrade --package V2 --upgrade-capability 1,1 --sender A --policy dep_only
module V3::base_module {

    const Y: u64 = 0;

    public struct X {
        field0: u64,
        field1: u64,
    }
    public fun public_fun(): u64 { Y }
}

// Fine to introduce a new constant with the additive policy
//# upgrade --package V2 --upgrade-capability 1,1 --sender A --policy additive
module V3::base_module {


    const T: u64 = 2;
    const Y: u64 = 1;
    const Z: u64 = 0;
    const A: u64 = 42;

    public struct X {
        field0: u64,
        field1: u64,
    }
    public fun public_fun(): u64 { Y }
    public fun public_fun2(): u64 { T }
}


// OK to remove constants in additive policy as long as they're unused
//# upgrade --package V3 --upgrade-capability 1,1 --sender A --policy additive
module V4::base_module {

    const Y: u64 = 1;
    const T: u64 = 2;
    const A: u64 = 42;

    public struct X {
        field0: u64,
        field1: u64,
    }
    public fun public_fun(): u64 { Y }
    public fun public_fun2(): u64 { T } 
}

// OK to remove constants in dep_only policy -- if they're unused
//# upgrade --package V4 --upgrade-capability 1,1 --sender A --policy dep_only
module V5::base_module {

    const Y: u64 = 1;
    const T: u64 = 2;

    public struct X {
        field0: u64,
        field1: u64,
    }
    public fun public_fun(): u64 { Y }
    public fun public_fun2(): u64 { T } 
}

// Fine to introduce a new constant as long as it's not used in either policy
//# upgrade --package V5 --upgrade-capability 1,1 --sender A --policy dep_only
module V6::base_module {

    const R: u64 = 3;
    const T: u64 = 2;
    const Y: u64 = 1;

    public struct X {
        field0: u64,
        field1: u64,
    }
    public fun public_fun(): u64 { Y }
    public fun public_fun2(): u64 { T }
}
