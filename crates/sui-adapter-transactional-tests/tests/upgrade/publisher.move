// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses A0=0x0 A1=0x0 --accounts A

//# publish --upgradeable --sender A
module A0::m {
    use sui::package;

    public struct A {}
    public struct M has drop {}

    fun init(m: M, ctx: &mut TxContext) {
        package::claim_and_keep(m, ctx);
    }
}

//# upgrade --package A0 --upgrade-capability 1,2 --sender A
module A1::m {
    use sui::package::{Self, Publisher};

    public struct A {}
    public struct B {}
    public struct M has drop {}

    fun init(m: M, ctx: &mut TxContext) {
        package::claim_and_keep(m, ctx);
    }

    entry fun test<T>(p: &Publisher) {
        assert!(package::from_package<T>(p), 0)
    }
}

//# run A1::m::test --type-args A0::m::A --args object(1,1) --sender A

//# run A1::m::test --type-args A1::m::B --args object(1,1) --sender A
