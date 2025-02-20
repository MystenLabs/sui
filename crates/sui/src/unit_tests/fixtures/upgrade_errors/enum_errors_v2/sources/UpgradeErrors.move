// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {

    public enum EnumAddAbility has copy, drop { // add drop
        A,
    }

    public enum EnumRemoveAbility has copy { // drop removed
        A,
    }

    public enum EnumAddAndRemoveAbility has copy, store { // change drop to store
        A,
    }

    public enum EnumAddVariant {
        A,
        B, // added
    }

    public enum EnumRemoveVariant {
        A,
        // B, removed
    }

    public enum EnumChangeVariant {
        A,
        C, // changed from B
    }

    public enum EnumChangeAndAddVariant {
        A,
        C, // to be changed to C
        D // added
    }

    public enum EnumChangeAndRemoveVariant {
        A,
        C, // changed to C
        // removed C,
    }

    // with types
    public enum EnumAddAbilityWithTypes has copy, drop { // drop added
        A { a: u8 },
    }

    public enum EnumRemoveAbilityWithTypes has copy { // drop removed
        A { a: u8 },
    }

    public enum EnumAddVariantWithTypes {
        A { a: u8 },
        B { b: u8 }, // added
    }

    public enum EnumRemoveVariantWithTypes {
        A { a: u8 },
        // B { b: u8 }, removed
    }

    public enum EnumChangeVariantWithTypes {
        A { a: u8 },
        C { b: u8 }, // changed to C
    }

    public enum EnumChangeAndAddVariantWithTypes {
        A { a: u8 },
        C { b: u8 }, // to be changed to C
        D { d: u8 }, // added
    }

    public enum EnumChangeAndRemoveVariantWithPositionalTypes {
        A(u8),
        C(u8), // changed to C
        // C(u8) removed
    }

    public enum EnumChangePositionalType {
        A(u8), // add u8
        B(u16), // changed to u16
        C(u8), // removed u8
        D, // removed u8 from variant
    }

    public struct ChangeFieldA {
        a: u32,
    }

    public struct ChangeFieldB {
        b: u32,
    }

    public enum EnumWithPositionalChanged {
        A(ChangeFieldB), // changed to ChangeFieldB
    }

    public enum EnumWithNamedChanged {
        A {
            x: ChangeFieldA,
            y: ChangeFieldA,
            z: ChangeFieldB, // changed to ChangeFieldB
        },
    }

}

