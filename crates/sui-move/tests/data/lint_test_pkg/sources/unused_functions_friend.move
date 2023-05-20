// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module lint_test_pkg::unused_functions_friend {
    use lint_test_pkg::unused_functions;

    public fun g() {
        unused_functions::used_friend()
    }
}
