// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::aborts {
    public fun only_abort() {
        abort;
    }

    public fun abort_with_code() {
        abort 5;
    }

    #[error]
    const EFoo: u64 = 9;
    public fun abort_with_const() {
        abort EFoo;
    }

    #[error(code=5)]
    const EBar: vector<u8> = b"The value is three";
    public fun abort_with_const_and_code() {
        abort EBar;
    }
}
