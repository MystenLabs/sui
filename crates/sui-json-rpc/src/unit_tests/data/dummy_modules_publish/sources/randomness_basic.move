// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Basic examples of how to work with Randomness objects.
module examples::randomness_basics {
    use sui::randomness;
    use sui::tx_context::TxContext;
    use sui::tx_context;

    /// A local (private) witness for associating Randomness objects to this specific module.
    struct WITNESS has drop {}

    /// Create a new Randomness object for the sender.
    public entry fun create_owned_randomness(ctx: &mut TxContext) {
        randomness::transfer(randomness::new(WITNESS {}, ctx), tx_context::sender(ctx));
    }

    /// Create a new shared Randomness object.
    public entry fun create_shared_randomness(ctx: &mut TxContext) {
        randomness::share_object(randomness::new(WITNESS {}, ctx));
    }

    /// After the object is created, the signature that is associated with this object can be retrieved from nodes.
    /// It then can be used for setting the object.
    /// After it is set, the random value can be read from object (see randomness::value).
    public entry fun set_randomness(rnd: &mut randomness::Randomness<WITNESS>, sig: vector<u8>) {
        // set can be called also from a function that sets it and immediately reads the randomness.
        randomness::set(rnd, sig);
    }
}
