// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::Proposal{
    use Std::Vector;

    struct Proposal has store {

        // the epoch this proposal is about
        epoch: u64,

        // minum weighted votes to reach quorum
        quorum_threshold: u64,

        // allowed voters
        allowed_voters: vector<address>,

        // voted voter
        voted: vector<address>,
    
        total_weighted_vote: u64,
    }

    public fun new(epoch: u64, quorum_threshold: u64, allowed_voters: vector<address>): Proposal {
        Proposal {
            epoch,
            quorum_threshold,
            allowed_voters: allowed_voters,
            voted: Vector::empty(),
            total_weighted_vote: 0,
        }
    }

    public fun vote(self: &mut Proposal, weight: u64, voter: address) {
        // is allowed voter
        assert!(cotains_address(&self.allowed_voters, voter), 0);

        // has not voted
        assert!(!cotains_address(&self.voted, voter), 0);

        Vector::push_back(&mut self.voted, voter);
        self.total_weighted_vote = self.total_weighted_vote + weight;
    }

    public fun has_reach_quorum(self: &Proposal): bool {
        self.total_weighted_vote >= self.quorum_threshold
    }

    public fun destroy(self: Proposal) {
        let Proposal {
            epoch:_,
            quorum_threshold: _,
            allowed_voters: _,
            voted: _,
            total_weighted_vote: _,
        } = self;
    }

    fun cotains_address(v: &vector<address>, target: address): bool {
        let length = Vector::length(v);
        let i = 0;
        while (i < length) {
            let candidate = Vector::borrow(v, i);
            if (*candidate == target) {
                return true
            };
            i = i + 1;
        };
        return false
    }

}