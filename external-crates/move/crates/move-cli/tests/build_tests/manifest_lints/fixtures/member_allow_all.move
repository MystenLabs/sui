// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[warn(all)]
module test::m {
    #[allow(all)]
    public fun warning() {
        let unused = 0u64;
    }
}
