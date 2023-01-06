// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module that allows creation and proof of publishing.
/// Based on the type name reflection; requires an OTW to claim
/// in the module initializer.
module sui::publisher {
    use sui::object::{Self, UID};
    use sui::tx_context::{TxContext, sender};
    use std::ascii::{Self, String};
    use std::type_name;
    use std::vector;
    use sui::types;

    /// Tried to claim ownership using a type that isn't a one-time witness.
    const ENotOneTimeWitness: u64 = 0;

    /// ASCII character code for the `:` (colon) symbol.
    const ASCII_COLON: u8 = 58;

    /// This type can only be created in the transaction that
    /// generates a module, by consuming its one-time witness, so it
    /// can be used to identify the address that published the package
    /// a type originated from.
    struct Publisher has key, store {
        id: UID,
        type: String
    }

    /// Claim a Publisher object.
    /// Requires a One-Time-Witness to
    public fun claim<OTW: drop>(otw: OTW, ctx: &mut TxContext): Publisher {
        assert!(types::is_one_time_witness(&otw), ENotOneTimeWitness);

        Publisher {
            id: object::new(ctx),
            type: type_name::into_string(type_name::get<OTW>()),
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
    public fun burn(publisher: Publisher) {
        let Publisher { id, type: _ } = publisher;
        object::delete(id);
    }

    /// Check whether type belongs to the same package as the publisher object.
    public fun is_package<T>(publisher: &Publisher): bool {
        let this = ascii::as_bytes(&publisher.type);
        let their = ascii::as_bytes(type_name::borrow_string(&type_name::get<T>()));

        let i = 0;

        // 40 bytes => length of the HEX encoded string
        while (i < 40) {
            if (vector::borrow<u8>(this, i) != vector::borrow<u8>(their, i)) {
                return false
            };

            i = i + 1;
        };

        true
    }

    /// Check whether a type belogs to the same module as the publisher object.
    public fun is_module<T>(publisher: &Publisher): bool {
        if (!is_package<T>(publisher)) {
            return false
        };

        let this = ascii::as_bytes(&publisher.type);
        let their = ascii::as_bytes(type_name::borrow_string(&type_name::get<T>()));

        // 42 bytes => length of the HEX encoded string + :: (double colon)
        let i = 42;
        loop {
            let left = vector::borrow<u8>(this, i);
            let right = vector::borrow<u8>(their, i);

            if (left == &ASCII_COLON && right == &ASCII_COLON) {
                return true
            };

            if (left != right) {
                return false
            };

            i = i + 1;
        }
    }

    #[test_only]
    /// Test-only function to claim a Publisher object bypassing OTW check.
    public fun test_claim<OTW: drop>(_: OTW, ctx: &mut TxContext): Publisher {
        Publisher {
            id: object::new(ctx),
            type: type_name::into_string(type_name::get<OTW>()),
        }
    }
}

#[test_only]
module sui::test_publisher {
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

        publisher::burn(pub);
        test::end(test);
    }

    #[test]
    fun test_is_module() {
        let test = test::begin(@0x1);
        let pub = publisher::test_claim(TEST_OTW {}, ctx(&mut test));

        assert!(publisher::is_module<CustomType>(&pub), 0);
        assert!(publisher::is_module<Scenario>(&pub) == false, 0);

        publisher::burn(pub);
        test::end(test);
    }
}
