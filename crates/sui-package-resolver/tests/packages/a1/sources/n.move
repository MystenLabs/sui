// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module a::n {
    public struct T0 {
        t: a::m::T1<u16, u32>,
        u: a::m::T2,
    }

    public struct T1 {
        t: a::m::T1<a::m::T3, u32>,
        u: a::m::T4,
    }

    public enum E0 {
        V0 {
            t: a::m::E1<u16, u32>,
            u: a::m::T2,
            l: a::m::E2,
        }
    }

    public enum E1 {
        V0 {
            t: a::m::T1<a::m::T3, u32>,
            u: a::m::T4,
            et: a::m::E1<a::m::E3, u32>,
            eu: a::m::E4,
        }
    }
}
