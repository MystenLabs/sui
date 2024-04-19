// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test_V0=0x0 Test_V1=0x0 --accounts A

//# publish --upgradeable --sender A
module Test_V0::base_module {
    public struct X {
        field0: u64,
        field1: u64,
    }

    public fun public_fun(): u64 { 0 }
}

//# upgrade --package Test_V0 --upgrade-capability 1,1 --sender A
module Test_V1::base_module {
    public enum X {
        V0 {
            field0: u64,
            field1: u64,
        }
    }

    public fun public_fun(): u64 { 0 }
}
