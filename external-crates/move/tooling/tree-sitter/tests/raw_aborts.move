// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module a::aborts;

fun test_unable_to_destroy_non_zero() {
    abort;

    abort abort abort;

    abort
}
