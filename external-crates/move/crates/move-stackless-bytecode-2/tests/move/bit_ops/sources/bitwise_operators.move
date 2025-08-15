// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bit_ops::bit_ops {

    public fun and(a: u8, b: u8): u8 {
        a & b
    }

    public fun or(a: u8, b: u8): u8 {
        a | b
    }

}
