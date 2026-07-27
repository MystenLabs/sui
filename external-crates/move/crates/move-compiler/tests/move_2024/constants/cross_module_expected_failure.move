// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Pins the '#[expected_failure(abort_code = ...)]' attribute path for constants: attribute
// references do not go through constant visibility checks, so a cross-module reference to a
// constant without 'public(package)' is accepted there (as it was before cross-module
// constants existed). Question: should attribute references eventually require
// 'public(package)' for consistency with term uses?

module 0x42::a {

const ENotFound: u64 = 5;

public fun fail() { abort ENotFound }

}

module 0x42::b {

#[test]
#[expected_failure(abort_code = 0x42::a::ENotFound)]
fun expect_cross_module_code() {
    0x42::a::fail()
}

}
