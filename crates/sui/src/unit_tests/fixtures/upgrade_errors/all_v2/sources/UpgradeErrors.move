// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {
    // struct missing
    // public struct StructToBeRemoved {}

    // struct ability mismatch (add)
    public struct StructAbilityMismatchAdd has copy {} // added the copy ability where none existed

    // struct field mismatch (remove)
    public struct StructAbilityMismatchRemove {} // removed the copy ability

    // struct field mismatch (change)
    public struct StructAbilityMismatchChange has drop {} // changed from drop to copy

    // struct type param mismatch
    public struct StructTypeParamMismatch<T> { a: T } // changed S to T

    // struct field mismatch (add)
    public struct StructFieldMismatchAdd {
        a: u64,
        b: u64,
        c: u64, // added
    }

    // struct field mismatch (remove)
    public struct StructFieldMismatchRemove {
        a: u64,
        // removed b: u64
    }

    // struct field mismatch (change)
    public struct StructFieldMismatchChange {
        a: u64,
        b: u8 // changed b from u64 to u8
    }

    // enum missing
    // public enum EnumToBeRemoved {}

    // enum ability mismatch (add)
    public enum EnumAbilityMismatchAdd has copy {
        A,
    }

    // enum ability mismatch (remove)
    public enum EnumAbilityMismatchRemove {
        A,
    }

    // enum ability mismatch (change)
    public enum EnumAbilityMismatchChange has drop {
        A,
    }

    // enum new variant
    public enum EnumNewVariant {
        A,
        B,
        C,
        D // new variant
    }

    // enum variant missing
    public enum EnumVariantMissing {
        A,
        // remove B,
    }

    // function missing public
    fun function_to_have_public_removed() {}

    // function missing friend
    fun function_to_have_friend_removed() {}

    // function missing entry

    // function signature mismatch (add)
    public fun function_add_arg(a: u64) {}

    // function signature mismatch (remove)
    public fun function_remove_arg() {}

    // function signature mismatch (change)
    public fun function_change_arg(a: u8) {} // now has u8 instead of u64
}
