// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module upgrades::upgrades {
    // enum
    public struct MyStruct has drop {
        a: u32,
    }

    public enum MyEnum {
        A(MyStruct),
    }

    // struct
    public struct MyNestedStruct has drop {
        a: MyStruct,
    }

    public struct MyStructWithGeneric<T> has drop {
        f: T,
    }

    public fun func_with_generic_struct_param(a: MyStructWithGeneric<MyStruct>): u64 {
        0
    }
}
