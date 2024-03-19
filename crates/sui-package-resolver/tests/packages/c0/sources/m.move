// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module c::m {
    #[allow(unused_field)]
    struct T0 {
        t: a::n::T0,
        u: a::n::T1,
        v: a::m::T2,
        w: a::m::T3,
        x: b::m::T0,
    }

    public fun foo() {}

    public(friend) fun bar(_t0: &T0, _t1: &mut a::n::T1): u64 { 42 }

    #[allow(unused_function)]
    fun baz(x: u8): (u16, u32) { ((x as u16), (x as u32)) }
}
