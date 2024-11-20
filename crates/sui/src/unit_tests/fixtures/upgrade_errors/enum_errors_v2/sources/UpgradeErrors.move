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

    public enum EnumAddAndRemoveAbility has copy, store {
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

}

