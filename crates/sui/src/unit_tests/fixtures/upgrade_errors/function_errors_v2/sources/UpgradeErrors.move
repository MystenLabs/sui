// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {
    // changed argument from u64 to u32
    public fun func_with_wrong_param(a: u32): u64 {
        0
    } 
    
    // changed return type from u64 to u32
    public fun func_with_wrong_return(): u32 {
        0
    }
    
    // changed argument from u64 to u32 and return type from u64 to u32
    public fun func_with_wrong_param_and_return(a: u32): u32 {
        0
    }
    
    // removed second argument
    public fun func_with_wrong_param_length(a: u64): u64 {
        0
    }
    
    // changed return type from (u64, u64) to u64
    public fun func_with_wrong_return_length(): u64 {
        0
    }

    public struct StructA has drop {
        x: u64
    }

    public struct StructB has drop {
        x: u32
    }

    // changed argument from A to B
    public fun func_with_wrong_struct_param(a: StructB): u64 {
        0
    }

    // changed from reference to value
    public fun ref_to_value(a: u32): u64 {
        0
    }

    // u32 as ref
    public fun value_to_ref(a: &u32): u64 {
        0
    }

    // mutable to immutable reference
    public fun mutable_to_immutable_ref(a: &u32): u64 {
        0
    }

    // immutable to mutable reference
    public fun immutable_to_mutable_ref(a: &mut u32): u64 {
        0
    }
}

