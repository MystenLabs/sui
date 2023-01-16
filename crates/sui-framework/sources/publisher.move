// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module that allows creation and proof of publishing.
/// Based on the type name reflection; requires an OTW to claim
/// in the module initializer.
module sui::publisher {
    use sui::object::{Self, UID};
    use sui::tx_context::{TxContext, sender};
    use std::ascii::String;
    use std::type_name;
    use sui::types;

    /// Tried to claim ownership using a type that isn't a one-time witness.
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
    /// Requires a One-Time-Witness to prove ownership. Due to this constraint
    /// there can be only one Publisher object per module but multiple per package (!).
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
    public fun burn(self: Publisher) {
        let Publisher { id, package: _, module_name: _ } = self;
        object::delete(id);
    }

    /// Check whether type belongs to the same package as the publisher object.
    public fun is_package<T>(self: &Publisher): bool {
        let type = type_name::get<T>();

        (type_name::get_address(&type) == self.package)
    }

    /// Check whether a type belogs to the same module as the publisher object.
    public fun is_module<T>(self: &Publisher): bool {
        let type = type_name::get<T>();

        (type_name::get_address(&type) == self.package)
            && (type_name::get_module(&type) == self.module_name)
    }

    /// Read the name of the module.
    public fun module_name(self: &Publisher): &String {
        &self.module_name
    }

    /// Read the package address string.
    public fun package(self: &Publisher): &String {
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

#[test_only]
module sui::test_publisher {
    use std::ascii;
    use sui::publisher;
    use sui::test_scenario::{Self as test, Scenario, ctx};

    /// OTW for the test_publisher module
    struct TEST_OTW has drop {}

    /// Type to compare against
    struct CustomType {}

    #[test]
    fun test_is_package() {
        let test = test::begin(@0x1);
        let pub = publisher::test_claim(TEST_OTW {}, ctx(&mut test));

        assert!(publisher::is_package<CustomType>(&pub), 0);
        assert!(publisher::is_package<Scenario>(&pub), 0);

        assert!(&ascii::string(b"0000000000000000000000000000000000000002") == publisher::package(&pub), 0);

        publisher::burn(pub);
        test::end(test);
    }

    #[test]
    fun test_is_module() {
        let test = test::begin(@0x1);
        let pub = publisher::test_claim(TEST_OTW {}, ctx(&mut test));

        assert!(publisher::is_module<CustomType>(&pub), 0);
        assert!(publisher::is_module<Scenario>(&pub) == false, 0);

        assert!(&ascii::string(b"test_publisher") == publisher::module_name(&pub), 0);

        publisher::burn(pub);
        test::end(test);
    }
}
