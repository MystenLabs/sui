// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module: Decentralized Voting DApp
/// This module implements a simple voting system on Sui blockchain
/// Users can create polls, vote on them, and view results
module voting::voting {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::string::{Self, String};
    use std::vector;
    use sui::event;

    /// Error codes
    const EInvalidOption: u64 = 0;
    const EAlreadyVoted: u64 = 1;
    const EPollNotActive: u64 = 2;

    /// Poll object - represents a voting poll
    struct Poll has key, store {
        id: UID,
        question: String,
        options: vector<String>,
        votes: vector<u64>,
        total_votes: u64,
        creator: address,
        is_active: bool,
    }

    /// Vote record - tracks who voted on which poll
    struct VoteReceipt has key {
        id: UID,
        poll_id: address,
        voter: address,
        option_index: u64,
    }

    /// Event emitted when a new poll is created
    struct PollCreated has copy, drop {
        poll_id: address,
        question: String,
        creator: address,
    }

    /// Event emitted when someone votes
    struct VoteCast has copy, drop {
        poll_id: address,
        voter: address,
        option_index: u64,
    }

    /// Create a new poll with a question and options
    public entry fun create_poll(
        question: vector<u8>,
        option1: vector<u8>,
        option2: vector<u8>,
        ctx: &mut TxContext
    ) {
        let poll_uid = object::new(ctx);
        let poll_id = object::uid_to_address(&poll_uid);

        let mut options = vector::empty<String>();
        vector::push_back(&mut options, string::utf8(option1));
        vector::push_back(&mut options, string::utf8(option2));

        let mut votes = vector::empty<u64>();
        vector::push_back(&mut votes, 0);
        vector::push_back(&mut votes, 0);

        let poll = Poll {
            id: poll_uid,
            question: string::utf8(question),
            options,
            votes,
            total_votes: 0,
            creator: tx_context::sender(ctx),
            is_active: true,
        };

        event::emit(PollCreated {
            poll_id,
            question: poll.question,
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
        let poll_uid = object::new(ctx);
        let poll_id = object::uid_to_address(&poll_uid);

        let mut options = vector::empty<String>();
        let mut votes = vector::empty<u64>();
        let options_len = vector::length(&options_data);

        let mut i = 0;
        while (i < options_len) {
            let option = vector::borrow(&options_data, i);
            vector::push_back(&mut options, string::utf8(*option));
            vector::push_back(&mut votes, 0);
            i = i + 1;
        };

        let poll = Poll {
            id: poll_uid,
            question: string::utf8(question),
            options,
            votes,
            total_votes: 0,
            creator: tx_context::sender(ctx),
            is_active: true,
        };

        event::emit(PollCreated {
            poll_id,
            question: poll.question,
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
        let poll_id = object::uid_to_address(&poll.id);

        // Increment vote count for selected option
        let vote_count = vector::borrow_mut(&mut poll.votes, option_index);
        *vote_count = *vote_count + 1;
        poll.total_votes = poll.total_votes + 1;

        // Create vote receipt for the voter
        let receipt = VoteReceipt {
            id: object::new(ctx),
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
        assert!(poll.creator == tx_context::sender(ctx), 0);
        poll.is_active = false;
    }

    /// Reopen a poll (only creator can reopen)
    public entry fun reopen_poll(
        poll: &mut Poll,
        ctx: &mut TxContext
    ) {
        assert!(poll.creator == tx_context::sender(ctx), 0);
        poll.is_active = true;
    }

    // === View functions ===

    /// Get poll question
    public fun get_question(poll: &Poll): String {
        poll.question
    }

    /// Get poll options
    public fun get_options(poll: &Poll): vector<String> {
        poll.options
    }

    /// Get poll votes
    public fun get_votes(poll: &Poll): vector<u64> {
        poll.votes
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
}
