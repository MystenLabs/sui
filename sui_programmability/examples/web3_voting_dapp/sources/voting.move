// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: Decentralized Voting DApp
/// This module implements a simple voting system on Sui blockchain
/// Users can create polls, vote on them, and view results
module voting::voting {
    use sui::object::{Self, ID, Info};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector;
    use sui::event;

    /// Error codes
    const EInvalidOption: u64 = 0;
    const EPollNotActive: u64 = 1;
    const ENotCreator: u64 = 2;
    const EInsufficientOptions: u64 = 3;

    /// Poll object - represents a voting poll
    struct Poll has key {
        info: Info,
        question: vector<u8>,
        options: vector<vector<u8>>,
        votes: vector<u64>,
        total_votes: u64,
        creator: address,
        is_active: bool,
    }

    /// Vote record - tracks who voted on which poll
    struct VoteReceipt has key {
        info: Info,
        poll_id: ID,
        voter: address,
        option_index: u64,
    }

    /// Event emitted when a new poll is created
    struct PollCreated has copy, drop {
        poll_id: ID,
        creator: address,
    }

    /// Event emitted when someone votes
    struct VoteCast has copy, drop {
        poll_id: ID,
        voter: address,
        option_index: u64,
    }

    /// Create a new poll with a question and two options
    public entry fun create_poll(
        question: vector<u8>,
        option1: vector<u8>,
        option2: vector<u8>,
        ctx: &mut TxContext
    ) {
        let info = object::new(ctx);
        let poll_id = *object::info_id(&info);

        let options = vector::empty<vector<u8>>();
        vector::push_back(&mut options, option1);
        vector::push_back(&mut options, option2);

        let votes = vector::empty<u64>();
        vector::push_back(&mut votes, 0);
        vector::push_back(&mut votes, 0);

        let poll = Poll {
            info,
            question,
            options,
            votes,
            total_votes: 0,
            creator: tx_context::sender(ctx),
            is_active: true,
        };

        event::emit(PollCreated {
            poll_id,
            creator: tx_context::sender(ctx),
        });

        transfer::share_object(poll);
    }

    /// Create a poll with multiple options
    public entry fun create_poll_multi(
        question: vector<u8>,
        options_data: vector<vector<u8>>,
        ctx: &mut TxContext
    ) {
        let options_len = vector::length(&options_data);
        assert!(options_len >= 2, EInsufficientOptions);

        let info = object::new(ctx);
        let poll_id = *object::info_id(&info);

        let options = vector::empty<vector<u8>>();
        let votes = vector::empty<u64>();

        let i = 0;
        while (i < options_len) {
            let option = vector::borrow(&options_data, i);
            vector::push_back(&mut options, *option);
            vector::push_back(&mut votes, 0);
            i = i + 1;
        };

        let poll = Poll {
            info,
            question,
            options,
            votes,
            total_votes: 0,
            creator: tx_context::sender(ctx),
            is_active: true,
        };

        event::emit(PollCreated {
            poll_id,
            creator: tx_context::sender(ctx),
        });

        transfer::share_object(poll);
    }

    /// Cast a vote on a poll
    public entry fun vote(
        poll: &mut Poll,
        option_index: u64,
        ctx: &mut TxContext
    ) {
        assert!(poll.is_active, EPollNotActive);
        assert!(option_index < vector::length(&poll.options), EInvalidOption);

        let voter = tx_context::sender(ctx);
        let poll_id = *object::info_id(&poll.info);

        // Increment vote count for selected option
        let vote_count = vector::borrow_mut(&mut poll.votes, option_index);
        *vote_count = *vote_count + 1;
        poll.total_votes = poll.total_votes + 1;

        // Create vote receipt for the voter
        let receipt = VoteReceipt {
            info: object::new(ctx),
            poll_id,
            voter,
            option_index,
        };

        event::emit(VoteCast {
            poll_id,
            voter,
            option_index,
        });

        transfer::transfer(receipt, voter);
    }

    /// Close a poll (only creator can close)
    public entry fun close_poll(
        poll: &mut Poll,
        ctx: &mut TxContext
    ) {
        assert!(poll.creator == tx_context::sender(ctx), ENotCreator);
        poll.is_active = false;
    }

    /// Reopen a poll (only creator can reopen)
    public entry fun reopen_poll(
        poll: &mut Poll,
        ctx: &mut TxContext
    ) {
        assert!(poll.creator == tx_context::sender(ctx), ENotCreator);
        poll.is_active = true;
    }

    // === View functions ===

    /// Get poll question
    public fun get_question(poll: &Poll): &vector<u8> {
        &poll.question
    }

    /// Get poll options count
    public fun get_options_count(poll: &Poll): u64 {
        vector::length(&poll.options)
    }

    /// Get specific option by index
    public fun get_option(poll: &Poll, index: u64): &vector<u8> {
        vector::borrow(&poll.options, index)
    }

    /// Get votes count for specific option
    public fun get_votes_for_option(poll: &Poll, index: u64): u64 {
        *vector::borrow(&poll.votes, index)
    }

    /// Get total votes
    public fun get_total_votes(poll: &Poll): u64 {
        poll.total_votes
    }

    /// Check if poll is active
    public fun is_active(poll: &Poll): bool {
        poll.is_active
    }

    /// Get poll creator
    public fun get_creator(poll: &Poll): address {
        poll.creator
    }

    #[test_only]
    /// Module initializer for tests
    public fun init_for_testing(ctx: &mut TxContext) {
        // Initialize module for testing
    }
}
