// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses M=0x0 A1=0x0 A2=0x0 B=0x0 B2=0x0 --accounts A

//# publish --upgradeable --sender A
module M::m {
    public fun g<T>() {}
}

//# publish --upgradeable --sender A
module A1::a {
    public struct Base()
}

//# upgrade --package A1 --upgrade-capability 2,1 --sender A
module A2::a {
    public struct Base()
    public fun new_fun() {}
}

//# set-address A2 object(3,0)

//# publish --upgradeable --sender A --dependencies A1
module B::b {
    fun f() {
    }
}

//# upgrade --package B --upgrade-capability 5,1 --sender A --dependencies A2
module B2::b {
    public struct BB<phantom T>()

    fun f() {
        A2::a::new_fun();
    }
}

//# programmable
//> M::m::g<sui::coin::Coin<A1::a::Base>>();
//> M::m::g<sui::coin::Coin<B2::b::BB<A2::a::Base>>>();
