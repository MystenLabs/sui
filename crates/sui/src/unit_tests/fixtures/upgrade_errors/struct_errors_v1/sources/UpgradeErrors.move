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


    // field mismatch
    public struct AddField {}
    // remove field
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
}

