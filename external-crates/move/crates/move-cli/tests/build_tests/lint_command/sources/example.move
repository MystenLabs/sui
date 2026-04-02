// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module test::example;

const EXAMPLE: u64 = 1;

fun t1(): u64 { return EXAMPLE }

#[allow(unused_function, lint(unneeded_return))]
fun t2(): u64 { return EXAMPLE }
