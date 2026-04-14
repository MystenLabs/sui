// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Rename::M2 {
    use Rename::M1::{Self, MyStruct, helper};

    public fun cross_file(): MyStruct {
        let _ = helper();
        M1::create()
    }
}
