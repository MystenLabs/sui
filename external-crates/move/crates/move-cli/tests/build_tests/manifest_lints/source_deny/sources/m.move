// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module test::m {
    #[deny(unused_variable)]
    public fun warning() {
        let unused = 0u64;
    }
}
