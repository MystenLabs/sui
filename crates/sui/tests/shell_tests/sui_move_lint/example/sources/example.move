// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::example;

const VALUE: u64 = 42;

// The `return` is unnecessary because the expression is already in tail position.
// This triggers the `unneeded_return` lint, which only runs at the `All` lint level
// that `sui move lint` enables; a plain `sui move build` does not report it.
public fun value(): u64 {
    return VALUE
}
