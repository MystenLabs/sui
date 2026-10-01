// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module test::m {
    public fun compiler_warning() {
        let unused = 0u64;
    }

    public fun first_lint() {
        abort 1
    }

    public fun second_lint(): u64 {
        return 0
    }
}
