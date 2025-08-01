// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses A1=0x0 A2=0x0 B=0x0 --accounts A

//# publish --upgradeable --sender A
module A1::a {
    public struct Base()
}

//# upgrade --package A1 --upgrade-capability 1,1 --sender A
module A2::a {
    public struct Base()
    public fun new_fun() {}
}

//# set-address A2 object(2,0)

//# publish --upgradeable --sender A --dependencies A1 A2
module B::b {
    fun f() {
    }
}
