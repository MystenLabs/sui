// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module c::m {
    struct T0 {
        t: a::n::T0,
        u: a::n::T1,
        v: a::m::T2,
        w: a::m::T3,
        x: b::m::T0,
    }
}
