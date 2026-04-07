// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module test::example;

const EXAMPLE: u64 = 1;

public fun t1(): u64 { return EXAMPLE }

#[allow(lint(unneeded_return))]
public fun t2(): u64 { return EXAMPLE }
