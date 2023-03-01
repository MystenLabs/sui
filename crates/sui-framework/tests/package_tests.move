// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::package_tests {
    use std::ascii;
    use std::hash;
    use sui::address;
    use sui::object::id_from_address as id;
    use sui::package;
    use sui::test_scenario::{Self as test, Scenario, ctx};

    /// OTW for the package_tests module -- it can't actually be a OTW
    /// (name matching module name) because we need to be able to
    /// create an instance of it in a test (outside a module initializer).
    struct TEST_OTW has drop {}

    /// Type to compare against
    struct CustomType {}

    #[test]
    fun test_from_package() {
        let test = test::begin(@0x1);
        let pub = package::test_claim(TEST_OTW {}, ctx(&mut test));

        assert!(package::from_package<CustomType>(&pub), 0);
        assert!(package::from_package<Scenario>(&pub), 0);
        assert!(&address::to_ascii_string(@0x2) == package::published_package(&pub), 0);

        package::burn_publisher(pub);
        test::end(test);
    }

    #[test]
    fun test_from_module() {
        let test = test::begin(@0x1);
        let pub = package::test_claim(TEST_OTW {}, ctx(&mut test));

        assert!(package::from_module<CustomType>(&pub), 0);
        assert!(package::from_module<Scenario>(&pub) == false, 0);

        assert!(&ascii::string(b"package_tests") == package::published_module(&pub), 0);

        package::burn_publisher(pub);
        test::end(test);
    }

    #[test]
    fun test_restrict_upgrade_policy() {
        let test = test::begin(@0x1);
        let cap = package::test_publish(id(@0x42), ctx(&mut test));

        assert!(package::upgrade_policy(&cap) == package::compatible_policy(), 0);
        package::only_additive_upgrades(&mut cap);
        assert!(package::upgrade_policy(&cap) == package::additive_policy(), 1);
        package::only_dep_upgrades(&mut cap);
        assert!(package::upgrade_policy(&cap) == package::dep_only_policy(), 2);
        package::make_immutable(cap);

        test::end(test);
    }

    #[test]
    fun test_full_upgrade_flow() {
        let test = test::begin(@0x1);
        let cap = package::test_publish(id(@0x42), ctx(&mut test));
        package::only_additive_upgrades(&mut cap);

        let version = package::version(&cap);
        let ticket = package::authorize_upgrade(
            &mut cap,
            package::dep_only_policy(),
            hash::sha3_256(b"package contents"),
        );

        let receipt = package::test_upgrade(ticket);
        package::commit_upgrade(&mut cap, receipt);
        assert!(package::version(&cap) == version + 1, 0);

        package::make_immutable(cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = sui::package::ETooPermissive)]
    fun test_failure_to_widen_upgrade_policy() {
        let test = test::begin(@0x1);
        let cap = package::test_publish(id(@0x42), ctx(&mut test));

        package::only_dep_upgrades(&mut cap);
        assert!(package::upgrade_policy(&cap) == package::dep_only_policy(), 1);

        package::only_additive_upgrades(&mut cap);
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = sui::package::ETooPermissive)]
    fun test_failure_to_authorize_overly_permissive_upgrade() {
        let test = test::begin(@0x1);
        let cap = package::test_publish(id(@0x42), ctx(&mut test));
        package::only_dep_upgrades(&mut cap);

        let _ticket = package::authorize_upgrade(
            &mut cap,
            package::compatible_policy(),
            hash::sha3_256(b"package contents"),
        );

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = sui::package::EAlreadyAuthorized)]
    fun test_failure_to_authorize_multiple_upgrades() {
        let test = test::begin(@0x1);
        let cap = package::test_publish(id(@0x42), ctx(&mut test));

        let _ticket0 = package::authorize_upgrade(
            &mut cap,
            package::compatible_policy(),
            hash::sha3_256(b"package contents 0"),
        );

        // It's an error to try and issue more than one simultaneous
        // upgrade ticket -- this should abort.
        let _ticket1 = package::authorize_upgrade(
            &mut cap,
            package::compatible_policy(),
            hash::sha3_256(b"package contents 1"),
        );

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = sui::package::EWrongUpgradeCap)]
    fun test_failure_to_commit_upgrade_to_wrong_cap() {
        let test = test::begin(@0x1);
        let cap0 = package::test_publish(id(@0x42), ctx(&mut test));
        let cap1 = package::test_publish(id(@0x43), ctx(&mut test));

        let ticket1 = package::authorize_upgrade(
            &mut cap1,
            package::dep_only_policy(),
            hash::sha3_256(b"package contents 1"),
        );

        let receipt1 = package::test_upgrade(ticket1);

        // Trying to update a cap with the receipt from some other cap
        // should fail with an abort.
        package::commit_upgrade(&mut cap0, receipt1);
        abort 0
    }
}
