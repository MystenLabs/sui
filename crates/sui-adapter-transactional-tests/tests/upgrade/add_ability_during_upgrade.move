// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test_V0=0x0 Test_V1=0x0 --accounts A

//# publish --upgradeable --sender A
module Test_V0::base {
    struct Foo {
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    use sui::object::UID;
    struct Foo has key {
        id: UID
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    struct Foo has drop { }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    struct Foo has copy { }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    struct Foo has store { }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    struct Foo has drop, store { }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    struct Foo has drop, copy { }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    struct Foo has drop, key { }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    use sui::object::UID;
    struct Foo has store, key {
        id: UID
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    struct Foo has store, copy { }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    struct Foo has drop, copy, store { }
}
