// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0

//# publish
module Test::f {
    public enum F has drop {
        V,
    }

    public fun test() {
        assert!(!sui::types::is_one_time_witness(&F::V));
    }
}

//# run Test::f::test
