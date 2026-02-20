// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module References::M1 {
    public struct MyStruct has drop {
        value: u64,
    }

    public fun create(): MyStruct {
        MyStruct { value: 42 }
    }

    public fun unoack(s: MyStruct): u64 {
        let MyStruct { value } = s;
        value
    }

    public fun dep() {
        Enums::int_match::int_match(7);
    }

    public fun alias() {
        use Enums::mut_match::match_mut as match_mut_fun;

        let mut v = 7;
        match_mut_fun(&mut v);
    }
}
