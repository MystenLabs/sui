// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {
    public fun func_with_wrong_param(a: u64): u64 {
        0
    } 

    public fun func_with_wrong_return(): u64 {
        0
    }
    
    public fun func_with_wrong_param_and_return(a: u64): u64 {
        0
    }
    
    public fun func_with_wrong_param_length(a: u64, b: u64): u64 {
        0
    }
    
    public fun func_with_wrong_return_length(): (u64, u64) {
        (0,0)
    }

    public struct StructA has drop {
        x: u64
    }

    public struct StructB has drop {
        x: u32
    }

    // change argument from A to B
    public fun func_with_wrong_struct_param(a: StructA): u64 {
        0
    }

    // change from reference to value
    public fun ref_to_value(a: &u32): u64 {
        0
    }

    // value to ref u32
    public fun value_to_ref(a: u32): u64 {
        0
    }

    // mutable to immutable reference
    public fun mutable_to_immutable_ref(a: &mut u32): u64 {
        0
    }

    // immutable to mutable reference
    public fun immutable_to_mutable_ref(a: &u32): u64 {
        0
    }
}
