// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(lint(all))]
module 0x0::m {
    public fun lint() {
        abort 1
    }
}
