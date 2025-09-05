// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests invalid unused values without copy

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct HotPotato {}
    public fun hot_potato(): HotPotato {
        HotPotato {}
    }
    public fun borrow_and_drop(_: HotPotato, _: &HotPotato) { abort 0 }
}

//# programmable --sender A
// unconsumed copyable value
//> 0: test::m1::hot_potato();
//> test::m1::borrow_and_drop(Result(0), Result(0));
