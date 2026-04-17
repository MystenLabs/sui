// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module RenameAlias::M1 {
    public struct MyStruct has drop {
        value: u64,
    }

    public fun create(): MyStruct {
        MyStruct { value: 42 }
    }
}
