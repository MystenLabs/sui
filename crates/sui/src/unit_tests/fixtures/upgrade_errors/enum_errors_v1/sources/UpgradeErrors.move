// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: UpgradeErrors

#[allow(unused_field)]
module upgrades::upgrades {

    public enum EnumAddAbility has copy { // add drop
        A,
    }

    public enum EnumRemoveAbility has copy, drop {
        A,
    }

    public enum EnumAddAndRemoveAbility has copy, drop {
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
}

