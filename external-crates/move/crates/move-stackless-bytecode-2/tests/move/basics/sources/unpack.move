// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module basics::unpack {
    public struct S has drop { a: bool, b: u64 }

    public fun unpack(s: S): (bool, u64) {
        let S { a, b } = s;
        (a, b)
    }
}
