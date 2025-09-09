// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator_settlement;

use sui::accumulator::{AccumulatorRoot, accumulator_key, U128, create_u128, destroy_u128};

const ENotSystemAddress: u64 = 0;
const EInvalidSplitAmount: u64 = 1;

use fun sui::accumulator_metadata::remove_accumulator_metadata as AccumulatorRoot.remove_metadata;
use fun sui::accumulator_metadata::create_accumulator_metadata as AccumulatorRoot.create_metadata;

// === Settlement storage types and entry points ===

/// Called by settlement transactions to ensure that the settlement transaction has a unique
/// digest.
#[allow(unused_function)]
fun settlement_prologue(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
}

#[allow(unused_function)]
fun settle_u128<T>(
    accumulator_root: &mut AccumulatorRoot,
    owner: address,
    merge: u128,
    split: u128,
    ctx: &mut TxContext,
) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    // Merge and split should be netted out prior to calling this function.
    assert!((merge == 0 ) != (split == 0), EInvalidSplitAmount);

    let name = accumulator_key<T>(owner);

    if (accumulator_root.has_accumulator<T, U128>(name)) {
        let is_zero = {
            let value: &mut U128 = accumulator_root.borrow_accumulator_mut(name);
            value.update(merge, split);
            value.is_zero()
        };

        if (is_zero) {
            let value = accumulator_root.remove_accumulator<T, U128>(name);
            destroy_u128(value);
            accumulator_root.remove_metadata<T>(owner);
        }
    } else {
        // cannot split if the field does not yet exist
        assert!(split == 0, EInvalidSplitAmount);
        let value = create_u128(merge);

        accumulator_root.add_accumulator(name, value);
        accumulator_root.create_metadata<T>(owner, ctx);
    };
}
