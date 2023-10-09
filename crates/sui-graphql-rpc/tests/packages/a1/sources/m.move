// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module a::m {
    struct T0 {
        b: bool,
        v: vector<T1<T2, u128>>,
    }

    struct T1<P0, P1> {
        a: address,
        p: P0,
        q: vector<P1>,
    }

    struct T2 {
        x: u8,
    }

    struct T3 {
        y: u16,
    }

    struct T4 {
        z: u32,
    }
}
