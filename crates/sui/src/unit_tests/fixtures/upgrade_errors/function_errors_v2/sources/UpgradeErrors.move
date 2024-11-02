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
}

