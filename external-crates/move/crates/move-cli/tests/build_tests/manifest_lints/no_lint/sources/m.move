// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module test::m {
    #[deny(lint(abort_without_constant))]
    public fun lint() {
        abort 1
    }
}
