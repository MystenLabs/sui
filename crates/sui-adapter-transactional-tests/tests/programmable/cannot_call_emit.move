// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests cannot call init with programmable transactions

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct A has copy, drop, store {}
    public fun a(): A { A {} }
}

//# programmable
//> 0: test::m1::a();
//> sui::event::emit<test::m1::A>(Result(0));

//# programmable
//> 0: test::m1::a();
// wrong type annotation doesn't matter
//> sui::event::emit<bool>(Result(0));

//# programmable
//> 0: test::m1::a();
// function doesn't exist
//> sui::event::does_not_exist<test::m1::A>(Result(0));
