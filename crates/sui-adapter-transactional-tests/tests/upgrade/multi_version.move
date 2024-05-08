// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test_V1=0x0 Test_V2=0x0 Test_V3=0x0 --accounts A

//# publish --upgradeable --sender A
module Test_V1::M1 {
    fun init(_ctx: &mut TxContext) { }
}

//# upgrade --package Test_V1 --upgrade-capability 1,1 --sender A
module Test_V2::M1 {
    fun init(_ctx: &mut TxContext) { }
    public fun f1() { }
}

//# upgrade --package Test_V2 --upgrade-capability 1,1 --sender A
module Test_V3::M1 {
    fun init(_ctx: &mut TxContext) { }
}
