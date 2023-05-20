// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module lint_test_pkg::unused_functions {
    friend lint_test_pkg::unused_functions_friend;

    public fun f() {
        used_private()
    }

    fun unused_private() {}

    fun used_private() {}

    public(friend) fun used_friend() {}

    public(friend) fun unused_friend() {}
}
