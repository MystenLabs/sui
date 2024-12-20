// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module upgrades::upgrades {

    public struct StructToChange {
        new_field: u64 // change to u32
    }

    public struct StructToRemove {
        new_field: u64
    }

    public enum EnumToChange {
        A, // change to B
    }

    public enum EnumToRemove {
        A
    }

    // no public on functions
    fun function_to_change(): u64 { // change to u32 return
        0
    }

    fun function_to_remove(): u64 {
        0
    }
}