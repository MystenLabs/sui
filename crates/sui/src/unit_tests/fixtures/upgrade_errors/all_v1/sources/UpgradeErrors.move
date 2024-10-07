// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {
    // struct missing
    public struct StructToBeRemoved {
        b: u64
    }

    // struct ability mismatch (add)
    public struct StructAbilityMismatchAdd {}

    // struct ability mismatch (remove)
    public struct StructAbilityMismatchRemove has copy {}

    // struct ability mismatch (change)
    public struct StructAbilityMismatchChange has copy {}

    // struct type param mismatch
    public struct StructTypeParamMismatch<S, T> { a: S }

    // struct field mismatch (add)
    public struct StructFieldMismatchAdd {
        a: u64,
        b: u64
    }

    // struct field mismatch (remove)
    public struct StructFieldMismatchRemove {
        a: u64,
        b: u64
    }

    // struct field mismatch (change)
    public struct StructFieldMismatchChange {
        a: u64,
        b: u64
    }

    // enum missing
    public enum EnumToBeRemoved {
        A,
        B
    }

    // enum ability mismatch (add)
    public enum EnumAbilityMismatchAdd  {
        A,
    }

    // enum ability mismatch (remove)
    public enum EnumAbilityMismatchRemove has copy {
        A,
    }

    // enum ability mismatch (change)
    public enum EnumAbilityMismatchChange has copy {
        A,
    }

    // enum new variant
    public enum EnumNewVariant {
        A,
        B,
        C
    }

    // enum variant missing
    public enum EnumVariantMissing {
        A,
        B,
    }

    // function missing public
    public fun function_to_have_public_removed() {}

    // function missing friend
    public(package) fun function_to_have_friend_removed() {}

    // function missing entry


    // function signature mismatch (add)
    public fun function_add_arg() {}

    // function signature mismatch (remove)
    public fun function_remove_arg(a: u64) {}

    // function signature mismatch (change)
    public fun function_change_arg(a: u64) {}
}

