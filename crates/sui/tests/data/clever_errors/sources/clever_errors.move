// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Do _not_ edit this file (yes, even whitespace!). Editing this file will
// cause the tests that use this module to fail.
module clever_errors::clever_errors {
    #[error]
    const ENotFound: vector<u8> = b"Element not found in vector 💥 🚀 🌠";

    #[error]
    const ENotAString: vector<u64> = vector[1,2,3,4];

    public fun aborter() {
        abort 0
    }

    public fun aborter_line_no() {
        assert!(false);
    }

    public fun clever_aborter() {
        assert!(false, ENotFound);
    }

    public fun clever_aborter_not_a_string() {
        assert!(false, ENotAString);
    }
}
