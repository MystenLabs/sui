// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module c::m {
    #[allow(unused_field)]
    public struct T0 {
        t: a::n::T0,
        u: a::n::T1,
        v: a::m::T2,
        w: a::m::T3,
        x: b::m::T0,
    }

    public enum E0 {
        StructOnly {
            t: a::n::T0,
            u: a::n::T1,
            v: a::m::T2,
            w: a::m::T3,
            x: b::m::T0,
        },
        EnumsOnly {
            et: a::n::E0,
            eu: a::n::E1,
            ev: a::m::E2,
            ew: a::m::E3,
            ex: b::m::E0,
        },
        EnumsAndStructs {
            t: a::n::T0,
            u: a::n::T1,
            v: a::m::T2,
            w: a::m::T3,
            x: b::m::T0,
            et: a::n::E0,
            eu: a::n::E1,
            ev: a::m::E2,
            ew: a::m::E3,
            ex: b::m::E0,
        }
    }

    public fun foo() {}

    public(package) fun bar(_t0: &T0, _t1: &mut a::n::T1, _e0: &E0, _e1: &mut a::n::E1): u64 { 42 }

    #[allow(unused_function)]
    fun baz(x: u8): (u16, u32) { ((x as u16), (x as u32)) }
}
