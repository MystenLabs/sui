// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module b::m {
    use a::m::T2 as M;
    use a::n::T0 as N;

    public struct T0 {
        m: M,
        n: N,
    }
}
