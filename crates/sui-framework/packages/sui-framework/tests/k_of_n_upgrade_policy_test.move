// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::k_of_n_upgrade_policy_test {
    use sui::address::from_u256;
    use sui::k_of_n_upgrade_policy::{Self, KofNUpgradeCap, ProposedUpgrade, Ballot};
    use sui::object::{Self, id_from_address as id};
    use sui::package;
    use sui::vec_set::{Self, VecSet};
    use sui::test_scenario::{Self as test, Scenario, ctx};

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::ERequiredVotesError)]
    fun k_of_n_too_many_required_votes() {
        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(5, 30, &mut test);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::EAllowedVotersError)]
    fun k_of_n_too_many_voters() {
        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(101, 80, &mut test);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::EAllowedVotersError)]
    fun k_of_n_too_few_voters() {
        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(1, 1, &mut test);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        test::end(test);
    }

    #[test]
    fun k_of_n_voters_ok() {
        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(99, 70, &mut test);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        let k_of_n_cap = get_k_of_n_cap(50, 30, &mut test);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        let k_of_n_cap = get_k_of_n_cap(2, 1, &mut test);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        test::end(test);
    }

    #[test]
    fun k_of_n_restrict_upgrade_policy() {
        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(5, 3, &mut test);
        assert!(
            package::upgrade_policy(k_of_n_upgrade_policy::upgrade_cap(&k_of_n_cap)) 
                == package::compatible_policy(), 
            0,
        );
        k_of_n_upgrade_policy::only_additive_upgrades(&mut k_of_n_cap);
        assert!(
            package::upgrade_policy(k_of_n_upgrade_policy::upgrade_cap(&k_of_n_cap)) 
                == package::additive_policy(), 
            1,
        );
        k_of_n_upgrade_policy::only_dep_upgrades(&mut k_of_n_cap);
        assert!(
            package::upgrade_policy(k_of_n_upgrade_policy::upgrade_cap(&k_of_n_cap)) 
                == package::dep_only_policy(), 
            2,
        );
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        test::end(test);
    }

    #[test]
    fun k_of_n_propose_upgrade_ok() {
        let test = test::begin(@0x1);
        let digest: vector<u8> = x"0123456789";
        let k_of_n_cap = get_k_of_n_cap(5, 3, &mut test);

        test::next_tx(&mut test, @0x1);
        k_of_n_upgrade_policy::propose_upgrade(&k_of_n_cap, digest, ctx(&mut test));

        test::next_tx(&mut test, @0x1);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::EInvalidProposalForUpgrade)]
    fun k_of_n_authorize_upgrade_bad_cap() {
        let test = test::begin(@0x1);
        let digest: vector<u8> = x"0123456789";
        let k_of_n_cap = get_k_of_n_cap(5, 3, &mut test);

        test::next_tx(&mut test, @0x1);
        k_of_n_upgrade_policy::propose_upgrade(&k_of_n_cap, digest, ctx(&mut test));

        test::next_tx(&mut test, @0x1);
        let k_of_n_cap1 = get_k_of_n_cap(10, 6, &mut test);
        let proposal = test::take_shared<ProposedUpgrade>(&test);
        let ticket = k_of_n_upgrade_policy::authorize_upgrade(
            &mut k_of_n_cap1, 
            &mut proposal, 
            ctx(&mut test),
        );
        let receipt = package::test_upgrade(ticket);
        k_of_n_upgrade_policy::commit_upgrade(&mut k_of_n_cap, receipt);
        test::return_shared(proposal);

        test::next_tx(&mut test, @0x1);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap1);

        end_partial_test(k_of_n_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::ENotEnoughVotes)]
    fun k_of_n_authorize_upgrade_not_enough_votes() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(5, 3, &mut test);
        propose_upgrade(@0x1, &k_of_n_cap, digest, &mut test);
        perform_upgrade(@0x1, &mut k_of_n_cap, &mut test);
        end_partial_test(k_of_n_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::ENotEnoughVotes)]
    fun k_of_n_authorize_upgrade_not_enough_votes_1() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(5, 3, &mut test);
        propose_upgrade(@0x1, &k_of_n_cap, digest, &mut test);
        vote(@0x100, &mut test);
        perform_upgrade(@0x1, &mut k_of_n_cap, &mut test);
        end_partial_test(k_of_n_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::ENotEnoughVotes)]
    fun k_of_n_authorize_upgrade_not_enough_votes_2() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(2, 2, &mut test);
        propose_upgrade(@0x1, &k_of_n_cap, digest, &mut test);
        vote(@0x100, &mut test);
        perform_upgrade(@0x1, &mut k_of_n_cap, &mut test);
        end_partial_test(k_of_n_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::ENotEnoughVotes)]
    fun k_of_n_authorize_upgrade_not_enough_votes_3() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(9, 6, &mut test);
        propose_upgrade(@0x1, &k_of_n_cap, digest, &mut test);
        vote(@0x100, &mut test);
        vote(@0x101, &mut test);
        vote(@0x105, &mut test);
        vote(@0x106, &mut test);
        vote(@0x102, &mut test);
        perform_upgrade(@0x1, &mut k_of_n_cap, &mut test);
        end_partial_test(k_of_n_cap, test);
    }

    #[test]
    fun k_of_n_perform_upgrade_ok() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(5, 3, &mut test);
        propose_upgrade(@0x1, &k_of_n_cap, digest, &mut test);
        vote(@0x100, &mut test);
        destroy_ballot(@0x102, &mut test);
        vote(@0x101, &mut test);
        vote(@0x104, &mut test);
        perform_upgrade(@0x1, &mut k_of_n_cap, &mut test);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        destroy_ballot(@0x103, &mut test);
        test::end(test);

        let test = test::begin(@0x2);
        let k_of_n_cap = get_k_of_n_cap(10, 8, &mut test);
        propose_upgrade(@0x2, &k_of_n_cap, digest, &mut test);
        vote(@0x100, &mut test);
        vote(@0x101, &mut test);
        destroy_ballot(@0x102, &mut test);
        vote(@0x104, &mut test);
        vote(@0x105, &mut test);
        vote(@0x106, &mut test);
        vote(@0x107, &mut test);
        vote(@0x108, &mut test);
        vote(@0x109, &mut test);
        perform_upgrade(@0x2, &mut k_of_n_cap, &mut test);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        destroy_ballot(@0x103, &mut test);
        test::end(test);

        let test = test::begin(@0x3);
        let k_of_n_cap = get_k_of_n_cap(9, 3, &mut test);
        propose_upgrade(@0x3, &k_of_n_cap, digest, &mut test);
        vote(@0x100, &mut test);
        vote(@0x101, &mut test);
        destroy_ballot(@0x103, &mut test);
        vote(@0x104, &mut test);
        vote(@0x105, &mut test);
        destroy_ballot(@0x106, &mut test);
        perform_upgrade(@0x3, &mut k_of_n_cap, &mut test);
        destroy_ballot(@0x102, &mut test);
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        test::end(test);
    }

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::EInvalidBallot)]
    fun k_of_n_vote_bad_digest() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(5, 3, &mut test);
        propose_upgrade(@0x1, &k_of_n_cap, digest, &mut test);

        test::next_tx(&mut test, @0x104);
        let ballot = test::take_from_address<Ballot>(&test, @0x104);
        let proposal = test::take_shared<ProposedUpgrade>(&test);
        k_of_n_upgrade_policy::change_ballot_digest(&mut ballot, x"9876543210");
        k_of_n_upgrade_policy::vote(&mut proposal, ballot, ctx(&mut test));
        test::return_shared(proposal);

        perform_upgrade(@0x1, &mut k_of_n_cap, &mut test);
        end_partial_test(k_of_n_cap, test);
    }

    #[test]
    #[expected_failure(abort_code = sui::k_of_n_upgrade_policy::EInvalidBallot)]
    fun k_of_n_vote_wrong_proposal() {
        let digest: vector<u8> = x"0123456789";

        let test = test::begin(@0x1);
        let k_of_n_cap = get_k_of_n_cap(5, 3, &mut test);
        propose_upgrade(@0x1, &k_of_n_cap, digest, &mut test);

        test::next_tx(&mut test, @0x104);
        let ballot = test::take_from_address<Ballot>(&test, @0x104);
        let proposal = test::take_shared<ProposedUpgrade>(&test);
        let new_id = object::id_from_address(@0x22);
        k_of_n_upgrade_policy::change_ballot_proposed_upgrade(&mut ballot, new_id);
        k_of_n_upgrade_policy::vote(&mut proposal, ballot, ctx(&mut test));
        test::return_shared(proposal);

        perform_upgrade(@0x1, &mut k_of_n_cap, &mut test);
        end_partial_test(k_of_n_cap, test);
    }

    fun get_k_of_n_cap(
        voter_count: u256, 
        required_vote: u64, 
        test: &mut Scenario,
    ): KofNUpgradeCap {
        let cap = package::test_publish(id(@0x42), ctx(test));
        let voters = get_voters(voter_count);
        k_of_n_upgrade_policy::new(cap, required_vote, voters, ctx(test))
    }

    fun get_voters(count: u256): VecSet<address> {
        let voters = vec_set::empty();
        let voter = 0x100u256;
        while (voter < 0x100u256 + count) {
            vec_set::insert(&mut voters, from_u256(voter));
            voter = voter + 1;
        };
        voters
    }

    fun vote(voter: address, test: &mut Scenario) {
        test::next_tx(test, voter);
        let ballot = test::take_from_address<Ballot>(test, voter);
        let proposal = test::take_shared<ProposedUpgrade>(test);
        k_of_n_upgrade_policy::vote(&mut proposal, ballot, ctx(test));
        test::return_shared(proposal);
    }

    fun propose_upgrade(
        sender: address, 
        k_of_n_cap: &KofNUpgradeCap, 
        digest: vector<u8>,
        test: &mut Scenario,
    ) {
        test::next_tx(test, sender);
        k_of_n_upgrade_policy::propose_upgrade(k_of_n_cap, digest, ctx(test));
    }

    fun perform_upgrade(
        sender: address, 
        k_of_n_cap: &mut KofNUpgradeCap, 
        test: &mut Scenario,
    ) {
        test::next_tx(test, sender);
        let proposal = test::take_shared<ProposedUpgrade>(test);
        let ticket = k_of_n_upgrade_policy::authorize_upgrade(
            k_of_n_cap, 
            &mut proposal, 
            ctx(test),
        );
        let receipt = package::test_upgrade(ticket);
        k_of_n_upgrade_policy::commit_upgrade(k_of_n_cap, receipt);
        test::return_shared(proposal);
    }

    fun destroy_ballot(voter: address, test: &mut Scenario) {
        test::next_tx(test, voter);
        let ballot = test::take_from_address<Ballot>(test, voter);
        k_of_n_upgrade_policy::destroy_ballot(ballot, ctx(test));
    }

    fun end_partial_test(k_of_n_cap: KofNUpgradeCap, test: Scenario) {
        test::next_tx(&mut test, @0x1);
        let proposal = test::take_shared<ProposedUpgrade>(&test);
        k_of_n_upgrade_policy::discard_proposed_upgrade(proposal, ctx(&mut test));
        k_of_n_upgrade_policy::make_immutable(k_of_n_cap);
        test::end(test);
    }
}
