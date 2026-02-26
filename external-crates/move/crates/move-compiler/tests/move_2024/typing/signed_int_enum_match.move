// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    enum MyEnum has drop {
        Signed { val: i64 },
        Unsigned { val: u64 },
        Nothing,
    }

    fun create_signed(): MyEnum {
        MyEnum::Signed { val: 42i64 }
    }

    fun match_enum(e: MyEnum): i64 {
        match (e) {
            MyEnum::Signed { val } => val,
            MyEnum::Unsigned { val: _ } => 0i64,
            MyEnum::Nothing => -1i64,
        }
    }

    fun use_enum() {
        let e = create_signed();
        let _v = match_enum(e);
    }
}
