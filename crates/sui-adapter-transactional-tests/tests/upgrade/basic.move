// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A

//# publish --upgradeable --sender A
module Test::M1 {
    fun init(_ctx: &mut TxContext) { }
    public fun f1() { }
}

//# upgrade --package Test --upgrade-capability 1,1 --sender A
module Test::M1 {
    fun init(_ctx: &mut TxContext) { }
}
