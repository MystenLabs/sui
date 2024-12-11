// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::display_tests {
    use sui::test_scenario as test;
    use std::string::String;
    use sui::package;
    use sui::display;

    #[allow(unused_field)]
    /// An example object.
    /// Purely for visibility.
    public struct Capy has key {
        id: UID,
        name: String
    }

    /// Test witness type to create a Publisher object.
    public struct CAPY has drop {}

    #[test]
    fun capy_init() {
        let mut test = test::begin(@0x2);
        let pub = package::test_claim(CAPY {}, test.ctx());

        // create a new display object
        let mut display = display::new<Capy>(&pub, test.ctx());

        display.add(b"name".to_string(), b"Capy {name}".to_string());
        display.add(b"link".to_string(), b"https://capy.art/capy/{id}".to_string());
        display.add(b"image".to_string(), b"https://api.capy.art/capy/{id}/svg".to_string());
        display.add(b"description".to_string(), b"A Lovely Capy".to_string());

        pub.burn_publisher();
        transfer::public_transfer(display, @0x2);
        test.end();
    }
}
