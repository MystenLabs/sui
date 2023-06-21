// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// init with entry used to be allowed

//# init --addresses test=0x0 v2=0x0 --protocol-version 6 --accounts A

//# publish --sender A --upgradeable
module test::m {
    use sui::tx_context::TxContext;
    entry fun init(_: &mut TxContext) {
    }

    public fun foo() {}
}

//# run test::m::init

// TODO advance protocol version
// //# run test::m::init --protocol-version 7

// // m still loads
// //# run test::m::foo --protocol-version 7

// //# upgrade --package test  --sender A --upgrade-capability 1,1
// module v2::m {
//     use sui::tx_context::TxContext;
//     fun init(_: &mut TxContext) {
//     }

//     public fun foo() {}
// }
