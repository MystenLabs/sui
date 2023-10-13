
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
#[allow(unused_use)]
module sui::authenticator_state_tests {
    use std::string::{String, utf8};
    use std::vector;

    use sui::test_scenario::{Self, Scenario};
    use sui::authenticator_state::{
        Self,
        AuthenticatorState,
        create_active_jwk,
        update_authenticator_state_for_testing,
        get_active_jwks_for_testing,
        expire_jwks_for_testing,
        ActiveJwk,
    };
    use sui::tx_context;

    #[test]
    fun authenticator_state_tests_basic() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        authenticator_state::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let auth_state = test_scenario::take_shared<AuthenticatorState>(scenario);

        let jwk1 = create_active_jwk(utf8(b"iss1"), utf8(b"key1"), utf8(b"key1_payload"), 1);
        let jwk2 = create_active_jwk(utf8(b"iss1"), utf8(b"key2"), utf8(b"key2_payload"), 1);
        let jwk3 = create_active_jwk(utf8(b"iss1"), utf8(b"key3"), utf8(b"key3_payload"), 1);


        update_authenticator_state_for_testing(&mut auth_state, vector[jwk1, jwk3], test_scenario::ctx(scenario));

        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk1, 0);
        assert!(vector::borrow(&recorded_jwks, 1) == &jwk3, 0);

        update_authenticator_state_for_testing(&mut auth_state, vector[jwk2], test_scenario::ctx(scenario));
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk1, 0);
        assert!(vector::borrow(&recorded_jwks, 1) == &jwk2, 0);
        assert!(vector::borrow(&recorded_jwks, 2) == &jwk3, 0);

        expire_jwks_for_testing(&mut auth_state, 1, test_scenario::ctx(scenario));
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 3, 0);

        let jwk1 = create_active_jwk(utf8(b"iss1"), utf8(b"key1"), utf8(b"key1_payload"), 2);
        update_authenticator_state_for_testing(&mut auth_state, vector[jwk1], test_scenario::ctx(scenario));
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 3, 0);
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk1, 0);

        expire_jwks_for_testing(&mut auth_state, 2, test_scenario::ctx(scenario));
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 1, 0);
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk1, 0);

        test_scenario::return_shared(auth_state);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun authenticator_state_tests_deduplication() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        authenticator_state::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let auth_state = test_scenario::take_shared<AuthenticatorState>(scenario);

        let jwk1 = create_active_jwk(utf8(b"https://accounts.google.com"), utf8(b"kid2"), utf8(b"k1"), 0);
        update_authenticator_state_for_testing(&mut auth_state, vector[jwk1], test_scenario::ctx(scenario));

        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 1, 0);
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk1, 0);

        let jwk2 = create_active_jwk(utf8(b"https://www.facebook.com"), utf8(b"kid1"), utf8(b"k2"), 0);
        let jwk3 = create_active_jwk(utf8(b"https://accounts.google.com"), utf8(b"kid2"), utf8(b"k3"), 0);
        update_authenticator_state_for_testing(&mut auth_state, vector[jwk2, jwk3], test_scenario::ctx(scenario));

        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 2, 0);
        // jwk2 sorts before 1, and 3 is dropped because its a duplicated
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk2, 0);
        assert!(vector::borrow(&recorded_jwks, 1) == &jwk1, 0);

        let jwk4 = create_active_jwk(utf8(b"https://accounts.google.com"), utf8(b"kid4"), utf8(b"k4"), 0);
        update_authenticator_state_for_testing(&mut auth_state, vector[jwk4], test_scenario::ctx(scenario));
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 3, 0);
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk2, 0);
        assert!(vector::borrow(&recorded_jwks, 1) == &jwk1, 0);
        assert!(vector::borrow(&recorded_jwks, 2) == &jwk4, 0);

        test_scenario::return_shared(auth_state);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun authenticator_state_tests_expiry_edge_cases() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        authenticator_state::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let auth_state = test_scenario::take_shared<AuthenticatorState>(scenario);

        // expire on an empty state
        expire_jwks_for_testing(&mut auth_state, 1, test_scenario::ctx(scenario));

        let jwk1 = create_active_jwk(utf8(b"iss1"), utf8(b"key1"), utf8(b"key1_payload"), 1);
        let jwk2 = create_active_jwk(utf8(b"iss2"), utf8(b"key2"), utf8(b"key2_payload"), 1);
        let jwk3 = create_active_jwk(utf8(b"iss3"), utf8(b"key3"), utf8(b"key3_payload"), 1);

        update_authenticator_state_for_testing(
            &mut auth_state, vector[jwk1, jwk2, jwk3], test_scenario::ctx(scenario)
        );

        // because none of the issuers have a jwk in epoch 2, we expire nothing
        expire_jwks_for_testing(&mut auth_state, 2, test_scenario::ctx(scenario));
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 3, 0);

        // now add a new jwk for iss1 in epoch 2
        let jwk4 = create_active_jwk(utf8(b"iss1"), utf8(b"key4"), utf8(b"key4_payload"), 2);
        update_authenticator_state_for_testing(
            &mut auth_state, vector[jwk4], test_scenario::ctx(scenario)
        );
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 4, 0);

        // now iss2 has one jwk in epoch 2, so we expire the one from epoch 1
        expire_jwks_for_testing(&mut auth_state, 2, test_scenario::ctx(scenario));
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 3, 0);
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk4, 0);
        assert!(vector::borrow(&recorded_jwks, 1) == &jwk2, 0);
        assert!(vector::borrow(&recorded_jwks, 2) == &jwk3, 0);

        // now add two new keys in epoch 3
        let jwk5 = create_active_jwk(utf8(b"iss2"), utf8(b"key5"), utf8(b"key5_payload"), 3);
        let jwk6 = create_active_jwk(utf8(b"iss3"), utf8(b"key6"), utf8(b"key6_payload"), 3);
        update_authenticator_state_for_testing(
            &mut auth_state, vector[jwk5, jwk6], test_scenario::ctx(scenario)
        );
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 5, 0);
        assert!(vector::borrow(&recorded_jwks, 2) == &jwk5, 0);
        assert!(vector::borrow(&recorded_jwks, 4) == &jwk6, 0);

        // now iss2 and iss3 have one jwk in epoch 3, so we expire the one from epoch 1
        expire_jwks_for_testing(&mut auth_state, 3, test_scenario::ctx(scenario));
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 3, 0);
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk4, 0);
        assert!(vector::borrow(&recorded_jwks, 1) == &jwk5, 0);
        assert!(vector::borrow(&recorded_jwks, 2) == &jwk6, 0);

        expire_jwks_for_testing(&mut auth_state, 3, test_scenario::ctx(scenario));
        let recorded_jwks = get_active_jwks_for_testing(&auth_state, test_scenario::ctx(scenario));
        assert!(vector::length(&recorded_jwks) == 3, 0);
        assert!(vector::borrow(&recorded_jwks, 0) == &jwk4, 0);
        assert!(vector::borrow(&recorded_jwks, 1) == &jwk5, 0);
        assert!(vector::borrow(&recorded_jwks, 2) == &jwk6, 0);



        test_scenario::return_shared(auth_state);
        test_scenario::end(scenario_val);
    }
}
