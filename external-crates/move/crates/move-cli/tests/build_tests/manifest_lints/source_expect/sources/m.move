// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x0::m {
    #[expect(unused_variable)]
    public fun warning() {
        let unused = 0u64;
    }

    #[expect(lint(abort_without_constant))]
    public fun lint() {
        abort 1
    }
}
