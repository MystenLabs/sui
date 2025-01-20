// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {

    // ability mismatch
    public struct AddExtraAbility has copy {} // added copy
    public struct RemoveAbility has drop {} // removed copy
    public struct AddAndRemoveAbility has drop, store {} // remove copy, add store
    public struct RemoveMultipleAbilities has drop {} // remove copy, store
    public struct AddMultipleAbilities has drop, copy {}


    // add field to empty struct
    public struct AddFieldToEmpty {
        a: u64,
    }

    // add field
    public struct AddField {
        a: u64,
        b: u64, // added b
    }

    // remove field from struct with one field
    public struct RemoveOnlyField {
        // removed a: u64,
    }

    // remove field from struct with multiple fields
    public struct RemoveField {
        a: u64,
        // removed b: u64,
    }

    // change field name
    public struct ChangeFieldName {
        a: u64,
        c: u64, // changed from b to c
    }

    // change field type
    public struct ChangeFieldType {
        a: u64,
        b: u32, // changed to u32
    }

    // change field name and type
    public struct ChangeFieldNameAndType {
        a: u64,
        c: u32, // changed from b to c and u64 to u32
    }

    // add positional to empty positional struct
    public struct EmptyPositionalAdd(u64) // removed the u64

    // struct new positional
    public struct PositionalAdd(u64, u64, u64) // added a u64

    // struct field missing
    public struct PositionalRemove(u64, u64) // removed a u64

    // struct field mismatch
    public struct PositionalChange(u32, u64) // changed second u32 to u64

    // add named to empty positional struct
    public struct PositionalAddNamed{ a: u64 } // changed to named from empty positional

    // positional to named
    public struct PositionalToNamed{ a: u64 } // changed to named from positional

    // change positional to named and change type
    public struct PositionalToNamedAndChangeType{ a: u64 } // changed to named from positional and changed type to u64

    public struct ChangeFieldA {
        a: u32,
    }

    public struct ChangeFieldB {
        b: u32,
    }

    // change positional nested struct
    public struct ChangePositionalStruct (ChangeFieldB) // changed to ChangeFieldB

    // change named nested struct
    public struct ChangeNameNestedStruct {
        a: ChangeFieldB, // changed to ChangeFieldB
    }


    public struct NamedBox<A> { x: u32 }
    public struct NamedTwoBox<B, C> { x: u32, y: u32 }

    public struct NamedBoxInBox<D> { x: u32 }
    public struct NamedBoxInTwoBox<E, F> { x: u32 }

    public struct PositionalBox<G>(u32)
    public struct PositionalTwoBox<H, I>(u32, u32)

    public struct PositionalBoxInBox<J>(u32)
    public struct PositionalBoxInTwoBox<K, L>(u32)
}
