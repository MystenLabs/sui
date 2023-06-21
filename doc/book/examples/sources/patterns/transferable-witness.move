// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This pattern is based on a combination of two others: Capability and a Witness.
/// Since Witness is something to be careful with, spawning it should be allowed
/// only to authorized users (ideally only once). But some scenarios require
/// type authorization by module X to be used in another module Y. Or, possibly,
/// there's a case where authorization should be performed after some time.
///
/// For these rather rare scerarios, a storable witness is a perfect solution.
module examples::transferable_witness {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};

    /// Witness now has a `store` that allows us to store it inside a wrapper.
    struct WITNESS has store, drop {}

    /// Carries the witness type. Can be used only once to get a Witness.
    struct WitnessCarrier has key { id: UID, witness: WITNESS }

    #[allow(unused_function)]
    /// Send a `WitnessCarrier` to the module publisher.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            WitnessCarrier { id: object::new(ctx), witness: WITNESS {} },
            tx_context::sender(ctx)
        )
    }

    /// Unwrap a carrier and get the inner WITNESS type.
    public fun get_witness(carrier: WitnessCarrier): WITNESS {
        let WitnessCarrier { id, witness } = carrier;
        object::delete(id);
        witness
    }
}
