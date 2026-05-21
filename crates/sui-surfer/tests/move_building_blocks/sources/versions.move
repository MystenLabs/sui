// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises the `versioned` wrapper, including the remove/upgrade flow.
module move_building_blocks::versions {
    use sui::versioned::{Self, Versioned};

    public struct Inner has store {
        value: u64,
    }

    public struct InnerV2 has store {
        value: u64,
        extra: u64,
    }

    public struct Wrapper has key, store {
        id: UID,
        versioned: Versioned,
    }

    public fun create(value: u64, ctx: &mut TxContext) {
        let versioned = versioned::create(1, Inner { value }, ctx);
        transfer::share_object(Wrapper { id: object::new(ctx), versioned });
    }

    public fun upgrade(wrapper: &mut Wrapper, value: u64) {
        if (versioned::version(&wrapper.versioned) == 1) {
            let (Inner { value: old }, cap) =
                versioned::remove_value_for_upgrade<Inner>(&mut wrapper.versioned);
            versioned::upgrade(
                &mut wrapper.versioned,
                2,
                InnerV2 { value: old + (value % 1_000), extra: value % 1_000 },
                cap,
            );
        }
    }
}
