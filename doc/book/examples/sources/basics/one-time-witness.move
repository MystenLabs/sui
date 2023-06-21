// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This example illustrates how One Time Witness works.
///
/// One Time Witness (OTW) is an instance of a type which is guaranteed to
/// be unique across the system. It has the following properties:
///
/// - created only in module initializer
/// - named after the module (uppercased)
/// - cannot be packed manually
/// - has a `drop` ability
module examples::one_time_witness_registry {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use std::string::String;
    use sui::transfer;

    // This dependency allows us to check whether type
    // is a one-time witness (OTW)
    use sui::types;

    /// For when someone tries to send a non OTW struct
    const ENotOneTimeWitness: u64 = 0;

    /// An object of this type will mark that there's a type,
    /// and there can be only one record per type.
    struct UniqueTypeRecord<phantom T> has key {
        id: UID,
        name: String
    }

    /// Expose a public function to allow registering new types with
    /// custom names. With a `is_one_time_witness` call we make sure
    /// that for a single `T` this function can be called only once.
    public fun add_record<T: drop>(
        witness: T,
        name: String,
        ctx: &mut TxContext
    ) {
        // This call allows us to check whether type is an OTW;
        assert!(types::is_one_time_witness(&witness), ENotOneTimeWitness);

        // Share the record for the world to see. :)
        transfer::share_object(UniqueTypeRecord<T> {
            id: object::new(ctx),
            name
        });
    }
}

/// Example of spawning an OTW.
module examples::my_otw {
    use std::string;
    use sui::tx_context::TxContext;
    use examples::one_time_witness_registry as registry;

    /// Type is named after the module but uppercased
    struct MY_OTW has drop {}

    #[allow(unused_function)]
    /// To get it, use the first argument of the module initializer.
    /// It is a full instance and not a reference type.
    fun init(witness: MY_OTW, ctx: &mut TxContext) {
        registry::add_record(
            witness, // here it goes
            string::utf8(b"My awesome record"),
            ctx
        )
    }
}
