// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {
    // struct missing
    public fun func_with_wrong_param(a: u32): u64 {
        0
    } 
    
    public fun func_with_wrong_return(): u32 {
        0
    }
    
    public fun func_with_wrong_param_and_return(a: u32): u32 {
        0
    }
    
    public fun func_with_wrong_param_length(a: u64): u64 {
        0
    }
    
    public fun func_with_wrong_return_length(): u64 {
        0
    }

}

