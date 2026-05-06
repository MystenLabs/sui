// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Rename::M1 {
    public struct MyStruct has drop {
        value: u64,
    }

    public fun create(): MyStruct {
        MyStruct { value: 42 }
    }

    public fun unpack(s: MyStruct): u64 {
        let MyStruct { value } = s;
        value
    }

    public fun helper(): u64 { 42 }

    public fun call_helper(): u64 {
        helper() + helper()
    }

    const MY_CONST: u64 = 100;

    public fun use_const(): u64 {
        MY_CONST + MY_CONST
    }

    public fun locals() {
        let x = 1u64;
        let y = x + 2;
        let _z = x + y;
    }
}
