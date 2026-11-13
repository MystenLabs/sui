// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x0::m {
    #[allow(lint(abort_without_constant))]
    public fun lint() {
        abort 1
    }
}
