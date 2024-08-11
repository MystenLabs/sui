// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::package_tests {
    use sui::package::{Self, UpgradeCap, UpgradeTicket};
    use sui::test_utils;
    use sui::test_scenario::{Self, Scenario};

    /// OTW for the package_tests module -- it can't actually be a OTW
    /// (name matching module name) because we need to be able to
    /// create an instance of it in a test (outside a module initializer).
    public struct TEST_OTW has drop {}

    /// Type to compare against
    public struct CustomType {}

    #[test]
    fun test_from_package() {
        let mut scenario = test_scenario::begin(@0x1);
        let pub = package::test_claim(TEST_OTW {}, scenario.ctx());

        assert!(pub.from_package<CustomType>());
        assert!(pub.from_package<Scenario>());
        assert!(&@0x2.to_ascii_string() == pub.published_package());

        pub.burn_publisher();
        scenario.end();
    }

    #[test]
    fun test_from_module() {
        let mut scenario = test_scenario::begin(@0x1);
        let pub = package::test_claim(TEST_OTW {}, scenario.ctx());

        assert!(pub.from_module<CustomType>());
        assert!(pub.from_module<Scenario>() == false);

        assert!(&b"package_tests".to_ascii_string() == pub.published_module());

        pub.burn_publisher();
        scenario.end();
    }

    #[test]
    fun test_restrict_upgrade_policy() {
        let mut scenario = test_scenario::begin(@0x1);
        let mut cap = package::test_publish(@0x42.to_id(), scenario.ctx());

        assert!(cap.upgrade_policy() == package::compatible_policy());
        cap.only_additive_upgrades();
        assert!(cap.upgrade_policy() == package::additive_policy());
        cap.only_dep_upgrades();
        assert!(cap.upgrade_policy() == package::dep_only_policy());
        cap.make_immutable();

        scenario.end();
    }

    fun check_ticket(cap: &mut UpgradeCap, policy: u8, digest: vector<u8>): UpgradeTicket {
        let ticket = cap.authorize_upgrade(
            policy,
            digest,
        );
        test_utils::assert_eq(ticket.ticket_policy(), policy);
        test_utils::assert_ref_eq(ticket.ticket_digest(), &digest);
        ticket
    }

    #[test]
    fun test_upgrade_policy_reflected_in_ticket() {
        let mut scenario = test_scenario::begin(@0x1);
        let mut cap = package::test_publish(@0x42.to_id(), scenario.ctx());
        let mut policies = vector[
            package::dep_only_policy(),
            package::compatible_policy(),
            package::additive_policy(),
            // Add more policies here when they exist.
        ];

        while (!policies.is_empty()) {
            let policy = policies.pop_back();
            let ticket = check_ticket(&mut cap, policy, sui::hash::blake2b256(&vector[policy]));
            let receipt = ticket.test_upgrade();
            cap.commit_upgrade(receipt);
        };

        cap.make_immutable();
        scenario.end();
    }


    #[test]
    fun test_full_upgrade_flow() {
        let mut scenario = test_scenario::begin(@0x1);
        let mut cap = package::test_publish(@0x42.to_id(), scenario.ctx());
        cap.only_additive_upgrades();

        let version = cap.version();
        let ticket = cap.authorize_upgrade(
            package::dep_only_policy(),
            sui::hash::blake2b256(&b"package contents"),
        );

        test_utils::assert_eq(ticket.ticket_policy(), package::dep_only_policy());
        let receipt = ticket.test_upgrade();
        cap.commit_upgrade(receipt);
        assert!(cap.version() == version + 1);

        cap.make_immutable();
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = sui::package::ETooPermissive)]
    fun test_failure_to_widen_upgrade_policy() {
        let mut scenario = test_scenario::begin(@0x1);
        let mut cap = package::test_publish(@0x42.to_id(), scenario.ctx());

        cap.only_dep_upgrades();
        assert!(cap.upgrade_policy() == package::dep_only_policy());

        cap.only_additive_upgrades();
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = sui::package::ETooPermissive)]
    fun test_failure_to_authorize_overly_permissive_upgrade() {
        let mut scenario = test_scenario::begin(@0x1);
        let mut cap = package::test_publish(@0x42.to_id(), scenario.ctx());
        cap.only_dep_upgrades();

        let _ticket = cap.authorize_upgrade(
            package::compatible_policy(),
            sui::hash::blake2b256(&b"package contents"),
        );

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = sui::package::EAlreadyAuthorized)]
    fun test_failure_to_authorize_multiple_upgrades() {
        let mut scenario = test_scenario::begin(@0x1);
        let mut cap = package::test_publish(@0x42.to_id(), scenario.ctx());

        let _ticket0 = cap.authorize_upgrade(
            package::compatible_policy(),
            sui::hash::blake2b256(&b"package contents 0"),
        );

        // It's an error to try and issue more than one simultaneous
        // upgrade ticket -- this should abort.
        let _ticket1 = cap.authorize_upgrade(
            package::compatible_policy(),
            sui::hash::blake2b256(&b"package contents 1"),
        );

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = sui::package::EWrongUpgradeCap)]
    fun test_failure_to_commit_upgrade_to_wrong_cap() {
        let mut scenario = test_scenario::begin(@0x1);
        let mut cap0 = package::test_publish(@0x42.to_id(), scenario.ctx());
        let mut cap1 = package::test_publish(@0x43.to_id(), scenario.ctx());

        let ticket1 = cap1.authorize_upgrade(
            package::dep_only_policy(),
            sui::hash::blake2b256(&b"package contents 1"),
        );

        test_utils::assert_eq(ticket1.ticket_policy(), package::dep_only_policy());
        let receipt1 = ticket1.test_upgrade();

        // Trying to update a cap with the receipt from some other cap
        // should fail with an abort.
        cap0.commit_upgrade(receipt1);
        abort 0
    }
}
