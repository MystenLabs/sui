// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0

//# publish
module Test::f {
    use sui::event;

    public enum F has copy, drop {
        V1,
        V2(u64),
        V3(u64, u64),
        V4 { x: u64 },
    }

    public fun f1() {
        event::emit(F::V1);
    }

    public fun f2(x: u64) {
        event::emit(F::V2(x));
    }

    public fun f3(x: u64, y: u64) {
        event::emit(F::V3(x, y));
    }

    public fun f4(x: u64) {
        event::emit(F::V4 { x });
    }
}

//# run Test::f::f1

//# run Test::f::f2 --args 42

//# run Test::f::f3 --args 42 43

//# run Test::f::f4 --args 42
