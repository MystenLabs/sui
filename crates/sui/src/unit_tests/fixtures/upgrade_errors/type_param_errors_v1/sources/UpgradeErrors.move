// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

module upgrades::upgrades {
    // structs

    // change T to U (no effect)
    public struct StructTypeParamChange<T> has copy, drop { x : T }

    // add U
    public struct StructTypeParamAddU<T> has copy, drop { x : T }

    // remove copy constraint from T
    public struct StructTypeParamRemoveCopy<T: copy> has copy, drop { x : T }

    // remove drop constraint from T
    public struct StructTypeParamRemoveDrop<T: copy + drop> has copy, drop { x : T }

    // remove phantom from type param
    public struct StructTypeParamRemovePhantom<phantom T> has copy, drop { x : u64 }

    // add phantom to type param
    public struct StructTypeParamAddPhantom<T> has copy, drop { x : u64 }

    //enums
    // add U
    public enum EnumTypeParamAddU<T> has copy, drop {
        A(T),
    }

    // remove U
    public enum EnumTypeParamRemoveU<T, U> has copy, drop {
        A(T),
    }

    // remove constraint from T
    public enum EnumTypeParamRemoveCopy<T: copy> has copy, drop {
        A(T),
    }

    // functions

    // add type param
    public fun add_type_param<T>(a: T): T { return a }

    // remove type param
    public fun remove_type_param<T, U>(a: T): T { return a }

    // add constraint
    public fun add_constraint<T>(a: T): T { return a }

    // remove constraint (no effect)
    public fun remove_constraint<T: copy>(a: T): T { return a }

    // swap type params
    public fun swap_type_params<T: drop, U: drop + copy>(a: T, b: U): T { return a }
    
    // swap return type params
    public fun swap_type_params_return<T: drop, U: drop + copy>(a: T, b: U): T { return a }

    // change type on vector
    public fun vec_changed(_: vector<u32>) {}

    // change type param on vector
    public fun vec_changed_type_param<T: drop, U: drop + copy>(_: vector<T>) {}
}
