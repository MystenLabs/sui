// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Simple voting module for decentralized decision making.
///
/// Allows creating proposals that members can vote on.
/// Each address can vote once per proposal.
/// Proposals have a deadline and can be executed after voting ends.
module voting::simple_voting {
    use sui::object::{Self, Info, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::vec_map::{Self, VecMap};
    use std::vector;

    /// Error codes
    const ENotAuthorized: u64 = 0;
    const EAlreadyVoted: u64 = 1;
    const EVotingEnded: u64 = 2;
    const EVotingNotEnded: u64 = 3;
    const EAlreadyExecuted: u64 = 4;
    const EQuorumNotReached: u64 = 5;

    /// Voting choices
    const VOTE_YES: u8 = 1;
    const VOTE_NO: u8 = 2;
    const VOTE_ABSTAIN: u8 = 3;

    /// A voting proposal
    struct Proposal has key {
        id: UID,
        title: vector<u8>,
        description: vector<u8>,
        creator: address,
        yes_votes: u64,
        no_votes: u64,
        abstain_votes: u64,
        voters: VecMap<address, u8>,
        deadline: u64,
        executed: bool,
        quorum: u64, // Minimum votes needed
    }

    /// Create a new proposal
    public entry fun create_proposal(
        title: vector<u8>,
        description: vector<u8>,
        deadline: u64,
        quorum: u64,
        ctx: &mut TxContext
    ) {
        let proposal = Proposal {
            id: object::new(ctx),
            title,
            description,
            creator: tx_context::sender(ctx),
            yes_votes: 0,
            no_votes: 0,
            abstain_votes: 0,
            voters: vec_map::empty(),
            deadline,
            executed: false,
            quorum,
        };

        transfer::share_object(proposal);
    }

    /// Cast a vote on a proposal
    public entry fun vote(
        proposal: &mut Proposal,
        vote_choice: u8,
        ctx: &mut TxContext
    ) {
        let voter = tx_context::sender(ctx);

        // Check if voting is still open
        assert!(tx_context::epoch(ctx) < proposal.deadline, EVotingEnded);

        // Check if already voted
        assert!(!vec_map::contains(&proposal.voters, &voter), EAlreadyVoted);

        // Record the vote
        vec_map::insert(&mut proposal.voters, voter, vote_choice);

        // Update vote counts
        if (vote_choice == VOTE_YES) {
            proposal.yes_votes = proposal.yes_votes + 1;
        } else if (vote_choice == VOTE_NO) {
            proposal.no_votes = proposal.no_votes + 1;
        } else if (vote_choice == VOTE_ABSTAIN) {
            proposal.abstain_votes = proposal.abstain_votes + 1;
        };
    }

    /// Execute a proposal if it passed
    public entry fun execute_proposal(
        proposal: &mut Proposal,
        ctx: &mut TxContext
    ) {
        // Check voting has ended
        assert!(tx_context::epoch(ctx) >= proposal.deadline, EVotingNotEnded);

        // Check not already executed
        assert!(!proposal.executed, EAlreadyExecuted);

        // Check quorum reached
        let total_votes = proposal.yes_votes + proposal.no_votes + proposal.abstain_votes;
        assert!(total_votes >= proposal.quorum, EQuorumNotReached);

        // Mark as executed
        proposal.executed = true;

        // In a real implementation, this would trigger the proposed action
        // For this example, we just mark it as executed
    }

    /// View functions

    /// Get vote counts
    public fun get_votes(proposal: &Proposal): (u64, u64, u64) {
        (proposal.yes_votes, proposal.no_votes, proposal.abstain_votes)
    }

    /// Check if proposal passed
    public fun did_pass(proposal: &Proposal): bool {
        proposal.yes_votes > proposal.no_votes
    }

    /// Get total votes
    public fun total_votes(proposal: &Proposal): u64 {
        proposal.yes_votes + proposal.no_votes + proposal.abstain_votes
    }

    /// Check if address has voted
    public fun has_voted(proposal: &Proposal, voter: address): bool {
        vec_map::contains(&proposal.voters, &voter)
    }

    /// Check if voting is active
    public fun is_active(proposal: &Proposal, current_epoch: u64): bool {
        current_epoch < proposal.deadline && !proposal.executed
    }

    /// Get proposal info
    public fun get_info(proposal: &Proposal): (vector<u8>, vector<u8>, address, u64, bool) {
        (
            proposal.title,
            proposal.description,
            proposal.creator,
            proposal.deadline,
            proposal.executed
        )
    }
}
