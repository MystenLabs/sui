// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module Completion::test;

public enum SomeEnum has drop {
    SomeVariant,
}

public fun test() {
    ::Completion::test::SomeEnum::SomeVariant;
    ::Completion::test::test();
}
