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
}

