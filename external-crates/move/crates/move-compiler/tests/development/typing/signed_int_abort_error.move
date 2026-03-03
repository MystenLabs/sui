// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Abort with signed int should error (abort takes u64)
    fun abort_signed() {
        abort 1i64
    }

    fun abort_signed_var() {
        let code: i32 = 1i32;
        abort code
    }
}
