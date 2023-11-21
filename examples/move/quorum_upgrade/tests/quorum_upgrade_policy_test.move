// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module quorum_upgrade_policy::quorum_upgrade_policy_test {
    use quorum_upgrade_policy::quorum_upgrade_policy::{Self, QuorumUpgradeCap, ProposedUpgrade, VotingCap};
    use sui::address::from_u256;
    use sui::object::id_from_address as id;
    use sui::package;
    use sui::vec_set::{Self, VecSet};
    use sui::test_scenario::{Self as test, Scenario, ctx};

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::ERequiredVotesError)]
    fun quorum_upgrade_too_many_required_votes() {
        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(30, 5, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::EAllowedVotersError)]
    fun quorum_upgrade_too_many_voters() {
        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(80, 101, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::EAllowedVotersError)]
    fun quorum_upgrade_too_few_voters() {
        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(1, 0, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::ERequiredVotesError)]
    fun quorum_upgrade_too_few_required_votes() {
        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(0, 10, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    #[test]
    fun quorum_upgrade_voters_ok() {
        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(80, 80, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(100, 100, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(2, 2, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(70, 100, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(30, 50, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(1, 2, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    #[test]
    fun quorum_upgrade_restrict_upgrade_policy() {
        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        assert!(
            package::upgrade_policy(quorum_upgrade_policy::upgrade_cap(&quorum_upgrade_cap)) 
                == package::compatible_policy(), 
            0,
        );
        quorum_upgrade_policy::only_additive_upgrades(&mut quorum_upgrade_cap);
        assert!(
            package::upgrade_policy(quorum_upgrade_policy::upgrade_cap(&quorum_upgrade_cap)) 
                == package::additive_policy(), 
            1,
        );
        quorum_upgrade_policy::only_dep_upgrades(&mut quorum_upgrade_cap);
        assert!(
            package::upgrade_policy(quorum_upgrade_policy::upgrade_cap(&quorum_upgrade_cap)) 
                == package::dep_only_policy(), 
            2,
        );
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = sui::package::ETooPermissive)]
    fun quorum_upgrade_bad_upgrade_policy() {
        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        assert!(
            package::upgrade_policy(quorum_upgrade_policy::upgrade_cap(&quorum_upgrade_cap)) 
                == package::compatible_policy(), 
            0,
        );
        quorum_upgrade_policy::only_dep_upgrades(&mut quorum_upgrade_cap);
        assert!(
            package::upgrade_policy(quorum_upgrade_policy::upgrade_cap(&quorum_upgrade_cap)) 
                == package::dep_only_policy(), 
            1,
        );
        quorum_upgrade_policy::only_additive_upgrades(&mut quorum_upgrade_cap);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    #[test]
    fun quorum_upgrade_propose_upgrade_ok() {
        let test = test::begin(@0x1);
        let digest: vector<u8> = x"0123456789";
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);

        test::next_tx(&mut test, @0x1);
        quorum_upgrade_policy::propose_upgrade(&quorum_upgrade_cap, digest, ctx(&mut test));

        test::next_tx(&mut test, @0x1);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::EInvalidProposalForUpgrade)]
    fun quorum_upgrade_authorize_upgrade_bad_cap() {
        let test = test::begin(@0x1);
        let digest: vector<u8> = x"0123456789";
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);

        test::next_tx(&mut test, @0x1);
        quorum_upgrade_policy::propose_upgrade(&quorum_upgrade_cap, digest, ctx(&mut test));

        test::next_tx(&mut test, @0x1);
        let quorum_upgrade_cap1 = get_quorum_upgrade_cap(6, 10, &mut test);
        let proposal = test::take_shared<ProposedUpgrade>(&test);
        let ticket = quorum_upgrade_policy::authorize_upgrade(
            &mut quorum_upgrade_cap1, 
            &mut proposal, 
            ctx(&mut test),
        );
        let receipt = package::test_upgrade(ticket);
        quorum_upgrade_policy::commit_upgrade(&mut quorum_upgrade_cap, receipt);
        test::return_shared(proposal);

        test::next_tx(&mut test, @0x1);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap1);

        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::ENotEnoughVotes)]
    fun quorum_upgrade_authorize_upgrade_not_enough_votes() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::ENotEnoughVotes)]
    fun quorum_upgrade_authorize_upgrade_not_enough_votes_1() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        vote(@0x100, &mut test);
        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::ENotEnoughVotes)]
    fun quorum_upgrade_authorize_upgrade_not_enough_votes_2() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(2, 2, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        vote(@0x100, &mut test);
        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::ENotEnoughVotes)]
    fun quorum_upgrade_authorize_upgrade_not_enough_votes_3() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(6, 10, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        vote(@0x100, &mut test);
        vote(@0x101, &mut test);
        vote(@0x105, &mut test);
        vote(@0x106, &mut test);
        vote(@0x102, &mut test);
        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::ESignerMismatch)]
    fun quorum_upgrade_authorize_upgrade_bad_signer() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        let quorum_upgrade_cap_1 = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x2, &quorum_upgrade_cap_1, digest, &mut test);

        vote(@0x100, &mut test);
        vote(@0x103, &mut test);
        vote(@0x101, &mut test);
        vote(@0x102, &mut test);

        perform_upgrade(@0x1, &mut quorum_upgrade_cap_1, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap_1);
        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::EInvalidProposalForUpgrade)]
    fun quorum_upgrade_authorize_upgrade_bad_voter_cap() {
        let digest: vector<u8> = x"0123456789";
        let digest1: vector<u8> = x"9876543210";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        let quorum_upgrade_cap_1 = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x2, &quorum_upgrade_cap_1, digest1, &mut test);

        vote(@0x102, &mut test);
        vote(@0x103, &mut test);
        vote(@0x101, &mut test);

        perform_upgrade(@0x2, &mut quorum_upgrade_cap, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap_1);
        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::EAlreadyIssued)]
    fun quorum_upgrade_authorize_upgrade_already_issued() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);

        vote(@0x100, &mut test);
        vote(@0x103, &mut test);
        vote(@0x104, &mut test);

        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::EAlreadyIssued)]
    fun quorum_upgrade_vote_already_issued() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);

        vote(@0x100, &mut test);
        vote(@0x103, &mut test);
        vote(@0x104, &mut test);

        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        vote(@0x101, &mut test);
        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::EInvalidVoterForUpgrade)]
    fun quorum_upgrade_bad_voter() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        test::next_tx(&mut test, @0x100);
        // get the voter cap and use it over the next upgrade and proposal
        let voter_cap = test::take_from_address<VotingCap>(&test, @0x100);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        test::next_tx(&mut test, @0x100);
        let proposal = test::take_shared<ProposedUpgrade>(&test);
        quorum_upgrade_policy::vote(&mut proposal, &mut voter_cap, ctx(&mut test));
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::return_shared(proposal);
        test::return_to_address(@0x100, voter_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::EAlreadyVoted)]
    fun quorum_upgrade_vote_twice() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);

        vote(@0x100, &mut test);
        vote(@0x100, &mut test);

        end_partial_test(quorum_upgrade_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = quorum_upgrade_policy::quorum_upgrade_policy::EAlreadyIssued)]
    fun quorum_upgrade_upgrade_already_issued() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        vote(@0x100, &mut test);
        vote(@0x101, &mut test);
        vote(@0x104, &mut test);
        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        vote(@0x102, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    #[test]
    fun quorum_upgrade_perform_upgrade_ok() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        vote(@0x100, &mut test);
        vote(@0x101, &mut test);
        vote(@0x104, &mut test);
        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);

        let test = test::begin(@0x2);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(8, 10, &mut test);
        propose_upgrade(@0x2, &quorum_upgrade_cap, digest, &mut test);
        vote(@0x100, &mut test);
        vote(@0x101, &mut test);
        vote(@0x104, &mut test);
        vote(@0x105, &mut test);
        vote(@0x106, &mut test);
        vote(@0x107, &mut test);
        vote(@0x108, &mut test);
        vote(@0x109, &mut test);
        perform_upgrade(@0x2, &mut quorum_upgrade_cap, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);

        let test = test::begin(@0x3);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 9, &mut test);
        propose_upgrade(@0x3, &quorum_upgrade_cap, digest, &mut test);
        vote(@0x100, &mut test);
        vote(@0x101, &mut test);
        vote(@0x104, &mut test);
        vote(@0x105, &mut test);
        perform_upgrade(@0x3, &mut quorum_upgrade_cap, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);

        let test = test::begin(@0x4);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(1, 100, &mut test);
        propose_upgrade(@0x4, &quorum_upgrade_cap, digest, &mut test);
        vote(@0x140, &mut test);
        perform_upgrade(@0x4, &mut quorum_upgrade_cap, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);

        let test = test::begin(@0x5);
        let quorum_upgrade_cap = get_quorum_upgrade_cap(3, 5, &mut test);
        propose_upgrade(@0x1, &quorum_upgrade_cap, digest, &mut test);
        vote(@0x103, &mut test); 
        vote(@0x100, &mut test); 
        vote(@0x102, &mut test); 
        vote(@0x101, &mut test);
        vote(@0x104, &mut test);
        perform_upgrade(@0x1, &mut quorum_upgrade_cap, &mut test);
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }

    fun get_quorum_upgrade_cap(
        required_vote: u64, 
        voter_count: u256,
        test: &mut Scenario,
    ): QuorumUpgradeCap {
        let cap = package::test_publish(id(@0x42), ctx(test));
        let voters = get_voters(voter_count, 0x100);
        quorum_upgrade_policy::new(cap, required_vote, voters, ctx(test))
    }

    fun get_voters(count: u256, voter: u256): VecSet<address> {
        let voters = vec_set::empty();
        while (voter < 0x100u256 + count) {
            vec_set::insert(&mut voters, from_u256(voter));
            voter = voter + 1;
        };
        voters
    }

    fun vote(voter: address, test: &mut Scenario) {
        test::next_tx(test, voter);
        let voter_cap = test::take_from_address<VotingCap>(test, voter);
        let proposal = test::take_shared<ProposedUpgrade>(test);
        quorum_upgrade_policy::vote(&mut proposal, &mut voter_cap, ctx(test));
        test::return_to_address(voter, voter_cap);
        test::return_shared(proposal);
    }

    fun propose_upgrade(
        sender: address, 
        quorum_upgrade_cap: &QuorumUpgradeCap, 
        digest: vector<u8>,
        test: &mut Scenario,
    ) {
        test::next_tx(test, sender);
        quorum_upgrade_policy::propose_upgrade(quorum_upgrade_cap, digest, ctx(test));
    }

    fun perform_upgrade(
        sender: address, 
        quorum_upgrade_cap: &mut QuorumUpgradeCap, 
        test: &mut Scenario,
    ) {
        test::next_tx(test, sender);
        let proposal = test::take_shared<ProposedUpgrade>(test);
        let ticket = quorum_upgrade_policy::authorize_upgrade(
            quorum_upgrade_cap, 
            &mut proposal, 
            ctx(test),
        );
        let receipt = package::test_upgrade(ticket);
        quorum_upgrade_policy::commit_upgrade(quorum_upgrade_cap, receipt);
        test::return_shared(proposal);
    }

    fun end_partial_test(quorum_upgrade_cap: QuorumUpgradeCap, test: Scenario) {
        test::next_tx(&mut test, @0x1);
        let proposal = test::take_shared<ProposedUpgrade>(&test);
        quorum_upgrade_policy::discard_proposed_upgrade(proposal, ctx(&mut test));
        quorum_upgrade_policy::make_immutable(quorum_upgrade_cap);
        test::end(test);
    }
}
