// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various invalid usages of a borrowed arg

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has drop {}
    public fun r(): R { R{} }

    public fun take_and_imm(_: R, _: &R) { abort 0 }
    public fun imm_and_take(_: &R, _: R) { abort 0 }

    public fun take_and_mut(_: R, _: &mut R) { abort 0 }
    public fun mut_and_take(_: &mut R, _: R) { abort 0 }

    public fun imm_and_mut(_: &R, _: &mut R) { abort 0 }
    public fun mut_and_imm(_: &mut R, _: &R) { abort 0 }

    public fun imm_mut_imm(_:&R, _: &mut R, _: &R) { abort 0 }
    public fun imm_take_mut(_: &R, _: R, _: &mut R) { abort 0 }
}

//# programmable
//> 0: test::m1::r();
//> test::m1::take_and_imm(Result(0), Result(0))

//# programmable
//> 0: test::m1::r();
//> test::m1::imm_and_take(Result(0), Result(0))

//# programmable
//> 0: test::m1::r();
//> test::m1::take_and_mut(Result(0), Result(0))

//# programmable
//> 0: test::m1::r();
//> test::m1::mut_and_take(Result(0), Result(0))

//# programmable
//> 0: test::m1::r();
//> test::m1::imm_and_mut(Result(0), Result(0))

//# programmable
//> 0: test::m1::r();
//> test::m1::mut_and_imm(Result(0), Result(0))

//# programmable
//> 0: test::m1::r();
//> test::m1::imm_mut_imm(Result(0), Result(0), Result(0))

//# programmable
//> 0: test::m1::r();
//> test::m1::imm_take_mut(Result(0), Result(0), Result(0))
