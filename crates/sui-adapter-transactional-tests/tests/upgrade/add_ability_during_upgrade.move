// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test_V0=0x0 Test_V1=0x0 --accounts A --flavor core

//# publish --upgradeable --sender A
module Test_V0::base {
    public struct Foo {}
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    use sui::object::UID;

    public struct Foo has key {
        id: UID,
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public struct Foo has drop {}
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public struct Foo has copy {}
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public struct Foo has store {}
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public struct Foo has drop, store {}
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public struct Foo has copy, drop {}
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public struct Foo has key, drop {}
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    use sui::object::UID;

    public struct Foo has key, store {
        id: UID,
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public struct Foo has copy, store {}
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public struct Foo has copy, drop, store {}
}
