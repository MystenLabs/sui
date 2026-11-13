// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(all)]
module 0x0::m {
    #[deny(all)]
    public fun warning() {
        let unused = 0u64;
    }
}
