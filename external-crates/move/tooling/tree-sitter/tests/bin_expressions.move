// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module a::b {
    fun f() {
        a < *b && !c || (*&d == true);
    }
}
