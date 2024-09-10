// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test_V0=0x0 Test_V1=0x0 --accounts A

//# publish --upgradeable --sender A
module Test_V0::base {
    public enum Foo {
        V0
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public enum Foo {
        V0,
        V1,
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public enum Foo has drop {
        V0
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public enum Foo has copy {
        V0
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public enum Foo has store {
        V0
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public enum Foo has drop, store {
        V0
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public enum Foo has drop, copy {
        V0
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public enum Foo has store, copy {
        V0
    }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base {
    public enum Foo has drop, copy, store {
        V0
    }
}
