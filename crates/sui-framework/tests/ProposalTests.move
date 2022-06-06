// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::ProposalTests {
    use Sui::Proposal;

    #[test]
    #[expected_failure]
    public(script) fun test_not_allowed_vote() {
        let proposal = Proposal::new(0, 100, vector<address>[@0x42, @0x100]);
        Proposal::vote(&mut proposal, 1, @0x1);
        Proposal::destroy(proposal);
    }

    #[test]
    #[expected_failure]
    public(script) fun test_not_already_voted() {
        let proposal = Proposal::new(0, 100, vector<address>[@0x42, @0x100]);
        Proposal::vote(&mut proposal, 1, @0x42);
        Proposal::vote(&mut proposal, 1, @0x42);
        Proposal::destroy(proposal);
        
    }

    #[test]
    public(script) fun test_reach_quorum() {
        let proposal = Proposal::new(0, 50, vector<address>[@0x42, @0x100]);
        Proposal::vote(&mut proposal, 50, @0x42);
        assert!(Proposal::has_reach_quorum(&proposal), 1);
        Proposal::destroy(proposal);
    }
}
