// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::package_tests {
    use std::ascii;
    use sui::address;
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
}
