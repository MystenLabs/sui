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

    // field mismatch
    public struct AddField {
        a: u64,
        b: u64,
    }
    // remove field
    public struct RemoveField {
        a: u64,
        // b removed here
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
}

