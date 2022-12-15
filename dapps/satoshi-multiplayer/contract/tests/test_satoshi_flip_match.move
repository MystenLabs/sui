// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module contract::test_satoshi_flip_match{
    use std::hash::sha3_256;

    use sui::test_scenario;
    use sui::transfer;

    use contract::satoshi_flip_match::{Self, Match, ENotCorrectSecret, ENotMatchHost, ENotMatchGuesser, EMatchNotEnded};

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
        let round: u64 = 1;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world and sent to host
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            let match = satoshi_flip_match::create(host, guesser, round, ctx);
            transfer::transfer(match, host);
        };

        // check that host and guesser have been set correctly
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_address<Match>(scenario, host);
            assert!(satoshi_flip_match::host(&match) == host, ENotCorrectHostSet);
            assert!(satoshi_flip_match::guesser(&match) == guesser, ENotCorrectGuesserSet);
            test_scenario::return_to_sender(scenario, match);
        };


        // host places their hash
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let match_with_hash = satoshi_flip_match::set_hash(match, secret_hash, ctx);
            // transfer match to guesser
            transfer::transfer(match_with_hash, guesser);
        };

        // guesser places their guess
        test_scenario::next_tx(scenario, guesser);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let match_with_guess = satoshi_flip_match::set_guess(match, 1, ctx);
            transfer::transfer(match_with_guess, host)
        };

        // check if guess was submitted correctly
        test_scenario::next_tx(scenario, world);
        {
            let match = test_scenario::take_from_address<Match>(scenario, host);
            let submitted_guess = satoshi_flip_match::guess(&match);
            assert!(submitted_guess == 1, EWrongGuess);
            test_scenario::return_to_address(host, match);
        };

        // host reveals their secret
        test_scenario::next_tx(scenario,host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ended_match = satoshi_flip_match::reveal(match, secret);
            // transfer ended match back to world
            transfer::transfer(ended_match, world);
        };

        // make sure that house is actually winning
        test_scenario::next_tx(scenario, world);
        {
            let match = test_scenario::take_from_address(scenario, world);
            let winner = satoshi_flip_match::winner(&match);
            assert!( winner == host, EWrongWinner);
            test_scenario::return_to_address(world, match);
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
        let round: u64 = 1 ;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            let match = satoshi_flip_match::create(host, guesser, round, ctx);
            transfer::transfer(match, host);
        };


        // host places their hash
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let match_with_hash = satoshi_flip_match::set_hash(match, secret_hash, ctx);
            // transfer match to guesser
            transfer::transfer(match_with_hash, guesser);
        };

        // guesser places their guess
        test_scenario::next_tx(scenario, guesser);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let match_with_guess = satoshi_flip_match::set_guess(match, 1, ctx);
            transfer::transfer(match_with_guess, host)
        };

        // host reveals their secret with secret that does not match with hash
        test_scenario::next_tx(scenario,host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ended_match = satoshi_flip_match::reveal(match, wrong_secret);
            transfer::transfer(ended_match,world);
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
        let round: u64 = 1;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world and sent to host
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            let match = satoshi_flip_match::create(host, guesser, round, ctx);
            transfer::transfer(match, host);
        };


        // world places hash (instead of host)
        test_scenario::next_tx(scenario, world);
        {
            let match = test_scenario::take_from_address<Match>(scenario, host);
            let ctx = test_scenario::ctx(scenario);
            let match_with_hash = satoshi_flip_match::set_hash(match, secret_hash, ctx);
            // transfer match to guesser
            transfer::transfer(match_with_hash, guesser);
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
        let round: u64 = 1;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            let match = satoshi_flip_match::create(host, guesser, round, ctx);
            transfer::transfer(match, host);
        };


        // host places their hash
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let match_with_guess = satoshi_flip_match::set_hash(match, secret_hash, ctx);
            transfer::transfer(match_with_guess, guesser);
        };

        // world places their guess instead of guesser
        test_scenario::next_tx(scenario, world);
        {
            let match = test_scenario::take_from_address<Match>(scenario, guesser);
            let ctx = test_scenario::ctx(scenario);
            let match_with_guess = satoshi_flip_match::set_guess(match, 1, ctx);
            transfer::transfer(match_with_guess, host)
        };

        // host reveals their secret
        test_scenario::next_tx(scenario,host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ended_match = satoshi_flip_match::reveal(match, secret);
            transfer::transfer(ended_match, world);
        };

        test_scenario::end(scenario_val);
    }

    // tests for accessors

    // test for when trying to access winner while not set
    #[test]
    #[expected_failure(abort_code = EMatchNotEnded)]
    fun winner_not_set(){
        let world = @0x1EE7;
        let host = @0xAAA;
        let guesser = @0xBBB;

        let secret = b"topsecret";
        let secret_hash = sha3_256(secret);
        let round: u64 = 1;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;

        // match is created by world
        test_scenario::next_tx(scenario, world);
        {
            let ctx = test_scenario::ctx(scenario);
            let match = satoshi_flip_match::create(host, guesser, round, ctx);
            transfer::transfer(match, host);
        };


        // host places their hash
        test_scenario::next_tx(scenario, host);
        {
            let match = test_scenario::take_from_sender<Match>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let match_with_guess = satoshi_flip_match::set_hash(match, secret_hash, ctx);
            transfer::transfer(match_with_guess, guesser);
        };

        // guesser places their guess
        test_scenario::next_tx(scenario, guesser);
        {
            let match = test_scenario::take_from_address<Match>(scenario, guesser);
            let ctx = test_scenario::ctx(scenario);
            let match_with_guess = satoshi_flip_match::set_guess(match, 1, ctx);
            transfer::transfer(match_with_guess, host)
        };

        test_scenario::next_tx(scenario, world);
        {
            let match = test_scenario::take_from_address<Match>(scenario, host);
            let _ = satoshi_flip_match::winner(&match);
            test_scenario::return_to_sender(scenario, match);
        };

        test_scenario::end(scenario_val);

    }

}