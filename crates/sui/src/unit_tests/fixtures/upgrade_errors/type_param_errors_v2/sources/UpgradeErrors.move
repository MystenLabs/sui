// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

module upgrades::upgrades {
    // structs

    // changed to U (no effect)
    public struct StructTypeParamChange<U> has copy, drop { x : U }

    // added U
    public struct StructTypeParamAddU<T, U> has copy, drop { x : T }

    // removed copy constraint from T
    public struct StructTypeParamRemoveCopy<T> has copy, drop { x : T }

    // removed drop constraint from T
    public struct StructTypeParamRemoveDrop<T: copy> has copy, drop { x : T }

    // removed phantom
    public struct StructTypeParamRemovePhantom<T> has copy, drop { x : u64 }

    // added phantom
    public struct StructTypeParamAddPhantom<phantom T> has copy, drop { x : u64 }

    // enums
    // add U
    public enum EnumTypeParamAddU<T, U> has copy, drop {
        A(T),
    }

    // remove U
    public enum EnumTypeParamRemoveU<T> has copy, drop {
        A(T),
    }

    // removed constraint
    public enum EnumTypeParamRemoveCopy<T> has copy, drop {
        A(T),
    }

    // functions

    // type param added
    public fun add_type_param<T, U>(a: T): T { return a }

    // type param removed
    public fun remove_type_param<T>(a: T): T { return a }

    // constraint added
    public fun add_constraint<T: copy>(a: T): T { return a }

    // constraint removed (no effect)
    public fun remove_constraint<T>(a: T): T { return a }
}

