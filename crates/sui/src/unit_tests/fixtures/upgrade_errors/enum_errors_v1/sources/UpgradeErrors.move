// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {

    public enum EnumAddAbility has copy { // add drop
        A,
    }

    public enum EnumRemoveAbility has copy, drop { // remove drop
        A,
    }

    public enum EnumAddAndRemoveAbility has copy, drop { // change drop to store
        A,
    }

    public enum EnumAddVariant {
        A,
        // B, to be added
    }

    public enum EnumRemoveVariant {
        A,
        B, // to be removed
    }

    public enum EnumChangeVariant {
        A,
        B, // to be changed to C
    }

    public enum EnumChangeAndAddVariant {
        A,
        B, // to be changed to C
        // D, to be added
    }

    public enum EnumChangeAndRemoveVariant {
        A,
        B, // to be changed to C
        C, // to be removed
    }

    // with types
    public enum EnumAddAbilityWithTypes has copy { // add drop
        A { a: u8 },
    }

    public enum EnumRemoveAbilityWithTypes has copy, drop {
        A { a: u8 },
    }

    public enum EnumAddVariantWithTypes {
        A { a: u8 },
        // B { b: u8 }, to be added
    }

    public enum EnumRemoveVariantWithTypes {
        A { a: u8 },
        B { b: u8 }, // to be removed
    }

    public enum EnumChangeVariantWithTypes {
        A { a: u8 },
        B { b: u8 }, // to be changed to C
    }

    public enum EnumChangeAndAddVariantWithTypes {
        A { a: u8 },
        B { b: u8 }, // to be changed to C
        // D { d: u8 }, to be added
    }

    public enum EnumChangeAndRemoveVariantWithPositionalTypes {
        A(u8),
        B(u8), // to be changed to C
        C(u8), // to be removed
    }

    public enum EnumChangePositionalType {
        A, // add u8
        B(u8), // to be changed to u16
        C(u8, u8), // remove u8
        D(u8) // remove u8 from last variant
    }

    public struct ChangeFieldA {
        a: u32,
    }

    public struct ChangeFieldB {
        b: u32,
    }

    public enum EnumWithPositionalChanged {
        A(ChangeFieldA), // change to ChangeFieldB
    }

    public enum EnumWithNamedChanged {
        A {
            x: ChangeFieldA,
            y: ChangeFieldA,
            z: ChangeFieldA, // change to ChangeFieldB
        },
    }
}

