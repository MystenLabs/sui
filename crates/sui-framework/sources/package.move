// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Functions for operating on Move packages from within Move:
/// - Creating proof-of-publish objects from one-time witnesses
/// - Administering package upgrades through upgrade policies.
module sui::package {
    use sui::object::{Self, UID};
    use sui::tx_context::{TxContext, sender};
    use std::ascii::String;
    use std::type_name;
    use sui::types;

    /// Tried to create a `Publisher` using a type that isn't a
    /// one-time witness.
    const ENotOneTimeWitness: u64 = 0;

    /// This type can only be created in the transaction that
    /// generates a module, by consuming its one-time witness, so it
    /// can be used to identify the address that published the package
    /// a type originated from.
    struct Publisher has key, store {
        id: UID,
        package: String,
        module_name: String,
    }

    /// Claim a Publisher object.
    /// Requires a One-Time-Witness to prove ownership. Due to this
    /// constraint there can be only one Publisher object per module
    /// but multiple per package (!).
    public fun claim<OTW: drop>(otw: OTW, ctx: &mut TxContext): Publisher {
        assert!(types::is_one_time_witness(&otw), ENotOneTimeWitness);

        let type = type_name::get<OTW>();

        Publisher {
            id: object::new(ctx),
            package: type_name::get_address(&type),
            module_name: type_name::get_module(&type),
        }
    }

    /// Claim a Publisher object and send it to transaction sender.
    /// Since this function can only be called in the module initializer,
    /// the sender is the publisher.
    public fun claim_and_keep<OTW: drop>(otw: OTW, ctx: &mut TxContext) {
        sui::transfer::transfer(claim(otw, ctx), sender(ctx))
    }

    /// Destroy a Publisher object effectively removing all privileges
    /// associated with it.
    public fun burn_publisher(self: Publisher) {
        let Publisher { id, package: _, module_name: _ } = self;
        object::delete(id);
    }

    /// Check whether type belongs to the same package as the publisher object.
    public fun from_package<T>(self: &Publisher): bool {
        let type = type_name::get<T>();

        (type_name::get_address(&type) == self.package)
    }

    /// Check whether a type belongs to the same module as the publisher object.
    public fun from_module<T>(self: &Publisher): bool {
        let type = type_name::get<T>();

        (type_name::get_address(&type) == self.package)
            && (type_name::get_module(&type) == self.module_name)
    }

    /// Read the name of the module.
    public fun published_module(self: &Publisher): &String {
        &self.module_name
    }

    /// Read the package address string.
    public fun published_package(self: &Publisher): &String {
        &self.package
    }

    #[test_only]
    /// Test-only function to claim a Publisher object bypassing OTW check.
    public fun test_claim<OTW: drop>(_: OTW, ctx: &mut TxContext): Publisher {
        let type = type_name::get<OTW>();

        Publisher {
            id: object::new(ctx),
            package: type_name::get_address(&type),
            module_name: type_name::get_module(&type),
        }
    }
}
