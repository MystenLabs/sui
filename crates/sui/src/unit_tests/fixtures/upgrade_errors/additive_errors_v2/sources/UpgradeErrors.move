// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module upgrades::upgrades {

    public struct StructToChange {
        new_field: u32 // changed to u32
    }

    // public struct StructToRemove {}

    public enum EnumToChange {
        B, // changed to B
    }

    // public enum EnumToRemove {}

    fun function_to_change(): u32 { // changed to u32
        0
    }

    // fun function_to_remove(): u64 {}
}