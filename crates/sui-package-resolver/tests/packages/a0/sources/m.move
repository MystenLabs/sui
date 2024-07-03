// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module a::m {
    public struct T0 {
        b: bool,
        v: vector<T1<T2, u128>>,
    }

    public struct T1<P0, P1> {
        a: address,
        p: P0,
        q: vector<P1>,
    }

    public struct T2 {
        x: u8,
    }

    public enum E0 {
        V {
            b: bool,
            v: vector<E1<T1<T2, u128>, u128>>,
        }
    }

    public enum E1<P0, P1> {
        V {
            a: address,
            p: P0,
            q: vector<P1>,
        }
    }

    public enum E2 {
        V0 {
            x: u8,
        },
    }
}
