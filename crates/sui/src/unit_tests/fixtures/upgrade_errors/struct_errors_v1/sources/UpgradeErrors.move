// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {

    // ability mismatch
    public struct AddExtraAbility {}
    public struct RemoveAbility has copy, drop {}
    public struct AddAndRemoveAbility has copy, drop {}
    public struct RemoveMultipleAbilities has copy, drop, store {}
    public struct AddMultipleAbilities {}


    // add field to empty struct
    public struct AddFieldToEmpty {
        // add a,
    }

    // add fields
    public struct AddField {
        a: u32
        // b
    }

    // remove field from struct with one field
    public struct RemoveOnlyField {
        a: u64,
    }

    // remove field from struct with multiple fields
    public struct RemoveField {
        a: u64,
        b: u64, // remove this field
    }

    // change field name
    public struct ChangeFieldName {
        a: u64,
        b: u64, // change this field name to c
    }

    // change field type
    public struct ChangeFieldType {
        a: u64,
        b: u64, // change this field type to u32
    }

    // change field name and type
    public struct ChangeFieldNameAndType {
        a: u64,
        b: u64, // change field name to c and type to u32
    }

    // add positional to empty positional struct
    public struct EmptyPositionalAdd() // add u64

    // struct new positional
    public struct PositionalAdd(u64, u64) // add u64

    // struct field missing
    public struct PositionalRemove(u64, u64, u64) // remove u64

    // struct field mismatch
    public struct PositionalChange(u32, u32) // change second u32 to u64

    // add named to empty positional struct
    public struct PositionalAddNamed() // change to named { a: u64 }

    // change positional to named
    public struct PositionalToNamed(u64) // change to named { a: u64 }

    // change positional to named and change type
    public struct PositionalToNamedAndChangeType(u32) // change to named { a: u64 }

    public struct ChangeFieldA {
        a: u32,
    }

    public struct ChangeFieldB {
        b: u32,
    }

    // change positional nested struct
    public struct ChangePositionalStruct (ChangeFieldA) // change to ChangeFieldB

    // change named nested struct
    public struct ChangeNameNestedStruct {
        a: ChangeFieldA, // change to ChangeFieldB
    }

    // nested struct type param field mismatch
    public struct NamedBox<A> { x: A }
    public struct NamedTwoBox<B, C> { x: B, y: C }

    public struct NamedBoxInBox<D> { x: NamedBox<NamedBox<D>> }
    public struct NamedBoxInTwoBox<E, F> { x: NamedTwoBox<NamedBox<E>, NamedBox<F>> }

    public struct PositionalBox<G>(G)
    public struct PositionalTwoBox<H, I>(H, I)

    public struct PositionalBoxInBox<J>(PositionalBox<PositionalBox<J>>)
    public struct PositionalBoxInTwoBox<K, L>(PositionalTwoBox<PositionalBox<K>, PositionalBox<L>>)
}
