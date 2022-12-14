// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module contract::test_satoshi_flip_match{
    use std::hash::sha3_256;


    use sui::test_scenario;

    use contract::satoshi_flip_match::{Self, Match, Outcome, ENotCorrectSecret, ENotMatchHost, ENotMatchGuesser};

    const ENotCorrectHostSet: u64 = 0;
    const ENotCorrectGuesserSet: u64 = 1;
    const EWrongWinner: u64 = 2;
    const EWrongGuess: u64 = 3;

    #[test]
    fun host_wins_test(){
        let world = @0x1EE7;
        let host = @0xAAA;
        let guesser = @0xBBB;

        let secret = b"topsecret";
        let secret_hash = sha3_256(secret);

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::create(host, guesser, ctx);
        };

        // check that host and guesser have been set correctly
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            assert!(satoshi_flip_match::get_host(&match) == host, ENotCorrectHostSet);
            assert!(satoshi_flip_match::get_guesser(&match) == guesser, ENotCorrectGuesserSet);
            test_scenario::return_to_sender(scenario, match);
        };


        // host places their hash
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::set_hash(match, secret_hash, ctx);
        };

        // guesser places their guess
        test_scenario::next_tx(scenario, guesser);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::guess(match, 1, ctx);
        };

        // check if guess was submitted correctly
        test_scenario::next_tx(scenario, world);
        {
            let match = test_scenario::take_from_address<Match>(scenario, host);
            let submitted_guess = satoshi_flip_match::get_guess(&match);
            assert!(submitted_guess == 1, EWrongGuess);
            test_scenario::return_to_address(host, match);
        };

        // host reveals their secret
        test_scenario::next_tx(scenario,host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::reveal(match, secret, ctx);
        };

        // make sure that house is actually winning
        test_scenario::next_tx(scenario, world);
        {
            let outcome = test_scenario::take_shared<Outcome>(scenario);
            let winner = satoshi_flip_match::get_winner(&outcome);
            assert!( winner == host, EWrongWinner);
            test_scenario::return_shared(outcome);
        };

        test_scenario::end(scenario_val);
    }

    // test for when host posts secret that does not match hash
    #[test]
    #[expected_failure(abort_code = ENotCorrectSecret)]
    fun host_posts_wrong_secret(){
        let world = @0x1EE7;
        let host = @0xAAA;
        let guesser = @0xBBB;

        let secret = b"topsecret";
        let secret_hash = sha3_256(secret);
        let wrong_secret = b"wrongsecret";
        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::create(host, guesser, ctx);
        };


        // host places their hash
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::set_hash(match, secret_hash, ctx);
        };

        // guesser places their guess
        test_scenario::next_tx(scenario, guesser);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::guess(match, 1, ctx);
        };

        // host reveals their secret
        test_scenario::next_tx(scenario,host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::reveal(match, wrong_secret, ctx);
        };

        test_scenario::end(scenario_val);
    }

    // test for when host is not correct when setting hash
    #[test]
    #[expected_failure(abort_code = ENotMatchHost)]
    fun wrong_host_when_setting_hash(){
        let world = @0x1EE7;
        let host = @0xAAA;
        let guesser = @0xBBB;

        let secret = b"topsecret";
        let secret_hash = sha3_256(secret);
        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::create(host, guesser, ctx);
        };


        // world places hash (instead of host)
        test_scenario::next_tx(scenario, world);
        {
            let match = test_scenario::take_from_address<Match>(scenario, host);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::set_hash(match, secret_hash, ctx);
        };

        test_scenario::end(scenario_val);
    }

    // test for when host is not correct when revealing secret
    #[test]
    #[expected_failure(abort_code = ENotMatchHost)]
    fun wrong_host_when_revealing(){
        let world = @0x1EE7;
        let host = @0xAAA;
        let guesser = @0xBBB;

        let secret = b"topsecret";
        let secret_hash = sha3_256(secret);
        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::create(host, guesser, ctx);
        };


        // host places their hash
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::set_hash(match, secret_hash, ctx);
        };

        // guesser places their guess
        test_scenario::next_tx(scenario, guesser);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::guess(match, 1, ctx);
        };

        // world (instead of host) reveals their secret
        test_scenario::next_tx(scenario,world);
        {
            let match = test_scenario::take_from_address<Match>(scenario,host);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::reveal(match, secret, ctx);
        };

        test_scenario::end(scenario_val);
    }

    // test for when guesser is not correct
    #[test]
    #[expected_failure(abort_code = ENotMatchGuesser)]
    fun wrong_match_guesser(){
        let world = @0x1EE7;
        let host = @0xAAA;
        let guesser = @0xBBB;

        let secret = b"topsecret";
        let secret_hash = sha3_256(secret);
        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::create(host, guesser, ctx);
        };


        // host places their hash
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::set_hash(match, secret_hash, ctx);
        };

        // world places their guess instead of guesser
        test_scenario::next_tx(scenario, world);
        {
            let match = test_scenario::take_from_address<Match>(scenario, guesser);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::guess(match, 1, ctx);
        };

        // host reveals their secret
        test_scenario::next_tx(scenario,host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip_match::reveal(match, secret, ctx);
        };

        test_scenario::end(scenario_val);
    }



}