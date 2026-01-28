// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module use_math::testing;

#[test]
fun use_test() {
    std::debug::print(&math_a::math::a());
    std::debug::print(&math_b::math::a());
}
