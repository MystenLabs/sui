// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module voting::voting_tests {
    use voting::voting::{Self, Poll, VoteReceipt};
    use sui::test_scenario::{Self, Scenario};
    use sui::object;
    use std::vector;

    const ADMIN: address = @0xAD;
    const USER1: address = @0x1;
    const USER2: address = @0x2;
    const USER3: address = @0x3;

    // Helper function to get a test vector<u8> from string literal
    fun string_to_bytes(s: vector<u8>): vector<u8> {
        s
    }

    #[test]
    fun test_create_poll() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Create a poll with 2 options
        test_scenario::next_tx(scenario, &ADMIN);
        {
            voting::create_poll(
                string_to_bytes(b"Do you like Sui?"),
                string_to_bytes(b"Yes"),
                string_to_bytes(b"No"),
                test_scenario::ctx(scenario)
            );
        };

        // Check that poll was created and shared
        test_scenario::next_tx(scenario, &ADMIN);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);

            // Verify poll properties
            assert!(voting::get_total_votes(&poll) == 0, 0);
            assert!(voting::is_active(&poll), 1);
            assert!(voting::get_creator(&poll) == ADMIN, 2);
            assert!(voting::get_options_count(&poll) == 2, 3);

            test_scenario::return_shared(scenario, poll);
        };

        test_scenario::end(scenario);
    }

    #[test]
    fun test_create_poll_multi() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Create a poll with multiple options
        test_scenario::next_tx(scenario, &ADMIN);
        {
            let options = vector::empty<vector<u8>>();
            vector::push_back(&mut options, string_to_bytes(b"Option 1"));
            vector::push_back(&mut options, string_to_bytes(b"Option 2"));
            vector::push_back(&mut options, string_to_bytes(b"Option 3"));
            vector::push_back(&mut options, string_to_bytes(b"Option 4"));

            voting::create_poll_multi(
                string_to_bytes(b"Which is best?"),
                options,
                test_scenario::ctx(scenario)
            );
        };

        // Verify poll was created with correct number of options
        test_scenario::next_tx(scenario, &ADMIN);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);

            assert!(voting::get_options_count(&poll) == 4, 0);
            assert!(voting::get_total_votes(&poll) == 0, 1);

            test_scenario::return_shared(scenario, poll);
        };

        test_scenario::end(scenario);
    }

    #[test]
    fun test_vote() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Create a poll
        test_scenario::next_tx(scenario, &ADMIN);
        {
            voting::create_poll(
                string_to_bytes(b"Do you like Sui?"),
                string_to_bytes(b"Yes"),
                string_to_bytes(b"No"),
                test_scenario::ctx(scenario)
            );
        };

        // USER1 votes for option 0
        test_scenario::next_tx(scenario, &USER1);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::vote(&mut poll, 0, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        // Check vote was recorded
        test_scenario::next_tx(scenario, &USER1);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);

            assert!(voting::get_total_votes(&poll) == 1, 0);
            assert!(voting::get_votes_for_option(&poll, 0) == 1, 1);
            assert!(voting::get_votes_for_option(&poll, 1) == 0, 2);

            test_scenario::return_shared(scenario, poll);

            // Check receipt was created
            let receipt = test_scenario::take_owned<VoteReceipt>(scenario);
            test_scenario::return_owned(scenario, receipt);
        };

        test_scenario::end(scenario);
    }

    #[test]
    fun test_multiple_votes() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Create a poll
        test_scenario::next_tx(scenario, &ADMIN);
        {
            voting::create_poll(
                string_to_bytes(b"Do you like Sui?"),
                string_to_bytes(b"Yes"),
                string_to_bytes(b"No"),
                test_scenario::ctx(scenario)
            );
        };

        // USER1 votes for option 0
        test_scenario::next_tx(scenario, &USER1);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::vote(&mut poll, 0, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        // USER2 votes for option 1
        test_scenario::next_tx(scenario, &USER2);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::vote(&mut poll, 1, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        // USER3 votes for option 0
        test_scenario::next_tx(scenario, &USER3);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::vote(&mut poll, 0, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        // Check all votes were recorded correctly
        test_scenario::next_tx(scenario, &ADMIN);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);

            assert!(voting::get_total_votes(&poll) == 3, 0);
            assert!(voting::get_votes_for_option(&poll, 0) == 2, 1);
            assert!(voting::get_votes_for_option(&poll, 1) == 1, 2);

            test_scenario::return_shared(scenario, poll);
        };

        test_scenario::end(scenario);
    }

    #[test]
    fun test_close_poll() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Create a poll
        test_scenario::next_tx(scenario, &ADMIN);
        {
            voting::create_poll(
                string_to_bytes(b"Do you like Sui?"),
                string_to_bytes(b"Yes"),
                string_to_bytes(b"No"),
                test_scenario::ctx(scenario)
            );
        };

        // Close the poll
        test_scenario::next_tx(scenario, &ADMIN);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::close_poll(&mut poll, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        // Check poll is closed
        test_scenario::next_tx(scenario, &ADMIN);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            assert!(!voting::is_active(&poll), 0);
            test_scenario::return_shared(scenario, poll);
        };

        test_scenario::end(scenario);
    }

    #[test]
    fun test_reopen_poll() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Create and close a poll
        test_scenario::next_tx(scenario, &ADMIN);
        {
            voting::create_poll(
                string_to_bytes(b"Do you like Sui?"),
                string_to_bytes(b"Yes"),
                string_to_bytes(b"No"),
                test_scenario::ctx(scenario)
            );
        };

        test_scenario::next_tx(scenario, &ADMIN);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::close_poll(&mut poll, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        // Reopen the poll
        test_scenario::next_tx(scenario, &ADMIN);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::reopen_poll(&mut poll, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        // Check poll is active again
        test_scenario::next_tx(scenario, &ADMIN);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            assert!(voting::is_active(&poll), 0);
            test_scenario::return_shared(scenario, poll);
        };

        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = 1)] // EPollNotActive
    fun test_vote_on_closed_poll_fails() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Create and close a poll
        test_scenario::next_tx(scenario, &ADMIN);
        {
            voting::create_poll(
                string_to_bytes(b"Do you like Sui?"),
                string_to_bytes(b"Yes"),
                string_to_bytes(b"No"),
                test_scenario::ctx(scenario)
            );
        };

        test_scenario::next_tx(scenario, &ADMIN);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::close_poll(&mut poll, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        // Try to vote on closed poll (should fail)
        test_scenario::next_tx(scenario, &USER1);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::vote(&mut poll, 0, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = 0)] // EInvalidOption
    fun test_vote_invalid_option_fails() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Create a poll with 2 options
        test_scenario::next_tx(scenario, &ADMIN);
        {
            voting::create_poll(
                string_to_bytes(b"Do you like Sui?"),
                string_to_bytes(b"Yes"),
                string_to_bytes(b"No"),
                test_scenario::ctx(scenario)
            );
        };

        // Try to vote for option 5 (should fail)
        test_scenario::next_tx(scenario, &USER1);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::vote(&mut poll, 5, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = 2)] // ENotCreator
    fun test_non_creator_cannot_close_poll() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Create a poll as ADMIN
        test_scenario::next_tx(scenario, &ADMIN);
        {
            voting::create_poll(
                string_to_bytes(b"Do you like Sui?"),
                string_to_bytes(b"Yes"),
                string_to_bytes(b"No"),
                test_scenario::ctx(scenario)
            );
        };

        // Try to close poll as USER1 (should fail)
        test_scenario::next_tx(scenario, &USER1);
        {
            let poll = test_scenario::take_shared<Poll>(scenario);
            voting::close_poll(&mut poll, test_scenario::ctx(scenario));
            test_scenario::return_shared(scenario, poll);
        };

        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = 3)] // EInsufficientOptions
    fun test_create_poll_with_insufficient_options_fails() {
        let scenario = &mut test_scenario::begin(&ADMIN);

        // Try to create a poll with only 1 option (should fail)
        test_scenario::next_tx(scenario, &ADMIN);
        {
            let options = vector::empty<vector<u8>>();
            vector::push_back(&mut options, string_to_bytes(b"Only one option"));

            voting::create_poll_multi(
                string_to_bytes(b"Bad poll?"),
                options,
                test_scenario::ctx(scenario)
            );
        };

        test_scenario::end(scenario);
    }
}
