#[test_only]
module ExperimentalFramework::VoteTests {

    use std::bcs;
    use std::signer;
    use std::unit_test;
    use std::vector;
    use CoreFramework::DiemTimestamp;
    use ExperimentalFramework::Genesis;
    use ExperimentalFramework::Vote;

    struct TestProposal has store, copy, drop {
        test_data: u8,
    }

    fun get_proposer(): signer {
        vector::pop_back(&mut unit_test::create_signers_for_testing(1))
    }

    fun get_three_voters(): (signer, address, signer, address, signer, address) {
        let signers = &mut unit_test::create_signers_for_testing(3);
        let voter1 = vector::pop_back(signers);
        let voter2 = vector::pop_back(signers);
        let voter3 = vector::pop_back(signers);
        let voter1_address = signer::address_of(&voter1);
        let voter2_address = signer::address_of(&voter2);
        let voter3_address = signer::address_of(&voter3);
        (voter1, voter1_address, voter2, voter2_address, voter3, voter3_address)
    }

    fun vote_test_helper(
        dr: &signer,
        expiration_timestamp_secs: u64,
    ) : (signer, signer, signer, Vote::BallotID, TestProposal) {
        let (voter1, voter1_address, voter2, voter2_address, voter3, voter3_address) = get_three_voters();
        let approvers = vector::empty();
        vector::push_back(&mut approvers, Vote::new_weighted_voter(1, bcs::to_bytes(&voter1_address)));
        vector::push_back(&mut approvers, Vote::new_weighted_voter(1, bcs::to_bytes(&voter2_address)));
        vector::push_back(&mut approvers, Vote::new_weighted_voter(1, bcs::to_bytes(&voter3_address)));

        let (proposer, _addr, _addr_bcs) = ballot_setup(dr);
        let proposal = TestProposal {
            test_data: 1,
        };
        let ballot_id = Vote::create_ballot(
            &proposer, // ballot_account
            *(&proposal), // proposal
            b"test_proposal", // proposal_type
            2, // num_votes_required
            approvers, // allowed_voters
            expiration_timestamp_secs, // expiration_timestamp_secs
        );
        (voter1, voter2, voter3, ballot_id, proposal)
    }

    fun ballot_setup(dr: &signer): (signer, address, vector<u8>) {
        Genesis::setup(dr);
        let proposer = get_proposer();
        let addr = signer::address_of(&proposer);
        let addr_bcs = bcs::to_bytes(&addr);
        (proposer, addr, addr_bcs)
    }

    #[test(dr = @CoreResources)]
    fun create_ballot_success(dr: signer) {
        let (proposer, addr, addr_bcs) = ballot_setup(&dr);
        let ballot_id = Vote::create_ballot(
            &proposer,
            TestProposal {
                test_data: 1,
            },
            b"test_proposal",
            1,
            vector::singleton(Vote::new_weighted_voter(1, *(&addr_bcs))),
            10,
        );
        assert!(&ballot_id == &Vote::new_ballot_id(0, addr), 0);

        let ballot_id = Vote::create_ballot(
            &proposer,
            TestProposal {
                test_data: 1,
            },
            b"test_proposal",
            1,
            vector::singleton(Vote::new_weighted_voter(1, *(&addr_bcs))),
            10,
        );
        assert!(&ballot_id == &Vote::new_ballot_id(1, addr), 0);

        let ballot_id = Vote::create_ballot(
            &proposer,
            TestProposal {
                test_data: 1,
            },
            b"test_proposal",
            1,
            vector::singleton(Vote::new_weighted_voter(1, addr_bcs)),
            10,
        );
        assert!(&ballot_id == &Vote::new_ballot_id(2, addr), 0);
    }

    #[test(dr = @CoreResources)]
    #[expected_failure(abort_code = 263, location = Vote)]
    fun create_ballot_expired_timestamp(dr: signer) {
        let (proposer, _, addr_bcs) = ballot_setup(&dr);
        Vote::create_ballot(
            &proposer, // ballot_account
            TestProposal { // proposal
                test_data: 1,
            },
            b"test_proposal", // proposal_type
            1, // num_votes_required
            vector::singleton(Vote::new_weighted_voter(1, addr_bcs)), // allowed_voters
            0, // expiration_timestamp_secs
        );
    }

    #[test(vm = @VMReserved, dr = @CoreResources)]
    fun gc_internal(vm: signer, dr: signer) {
        let (proposer, addr, addr_bcs) = ballot_setup(&dr);
        let _ballot_id1 = Vote::create_ballot(
            &proposer,
            TestProposal {
                test_data: 1,
            },
            b"test_proposal",
            1,
            vector::singleton(Vote::new_weighted_voter(1, *(&addr_bcs))),
            1,
        );

        let _ballot_id2 = Vote::create_ballot(
            &proposer,
            TestProposal {
                test_data: 1,
            },
            b"test_proposal",
            1,
            vector::singleton(Vote::new_weighted_voter(1, *(&addr_bcs))),
            2,
        );

        let _ballot_id3 = Vote::create_ballot(
            &proposer,
            TestProposal {
                test_data: 1,
            },
            b"test_proposal",
            1,
            vector::singleton(Vote::new_weighted_voter(1, *(&addr_bcs))),
            3,
        );

        let _ballot_id4 = Vote::create_ballot(
            &proposer,
            TestProposal {
                test_data: 1,
            },
            b"test_proposal",
            1,
            vector::singleton(Vote::new_weighted_voter(1, addr_bcs)),
            4,
        );

        DiemTimestamp::update_global_time(&vm, @0xCAFE, 3000000);
        let remove_ballots = Vote::gc_test_helper<TestProposal>(addr);
        assert!(vector::length(&remove_ballots) == 2, 0);
        assert!(&vector::pop_back(&mut remove_ballots) == &Vote::new_ballot_id(1, addr), 0);
        assert!(&vector::pop_back(&mut remove_ballots) == &Vote::new_ballot_id(0, addr), 0);
    }

    // TODO: test disabled due to timeout
    /*
        #[test(dr = @CoreResources)]
        #[expected_failure(abort_code = 520, location = Vote)]
        fun create_ballots_too_many(dr: signer) {
            let (proposer, _, addr_bcs) = ballot_setup(&dr);
            let i = 0;
            // limit is 256
            while (i <= 257) {
                Vote::create_ballot(
                    &proposer, // ballot_account
                    TestProposal { // proposal
                        test_data: 1,
                    },
                    b"test_proposal", // proposal_type
                    1, // num_votes_required
                    Vector::singleton(Vote::new_weighted_voter(1, *(&addr_bcs))), // allowed_voters
                    10, // expiration_timestamp_secs
                );
                i = i + 1;
            }
        }
    */

    #[test(dr = @CoreResources)]
    #[expected_failure(abort_code = 769, location = Vote)]
    fun remove_ballot(dr: signer) {
        let (voter1, _voter2, _voter3, ballot_id, proposal) = vote_test_helper(&dr, 10);
        Vote::remove_ballot_internal<TestProposal>(get_proposer(), *(&ballot_id));
        // Vote fails because there is no ballot
        Vote::vote(&voter1, *(&ballot_id), b"test_proposal", *(&proposal));
    }

    #[test(dr = @CoreResources)]
    #[expected_failure(abort_code = 769, location = Vote)]
    fun vote_simple(dr: signer) {
        let (voter1, voter2, voter3, ballot_id, proposal) = vote_test_helper(&dr, 10);
        // First vote does not approve the ballot
        assert!(!Vote::vote(&voter1, *(&ballot_id), b"test_proposal", *(&proposal)), 0);
        // Second vote approves the ballot
        assert!(Vote::vote(&voter2, *(&ballot_id), b"test_proposal", *(&proposal)), 0);
        // Third vote aborts
        Vote::vote(&voter3, *(&ballot_id), b"test_proposal", *(&proposal));
    }

    #[test(dr = @CoreResources)]
    fun vote_weighted(dr: signer) {
        let (voter1, voter1_address, voter2, voter2_address, _voter3, voter3_address) = get_three_voters();
        let approvers = vector::empty();
        vector::push_back(&mut approvers, Vote::new_weighted_voter(3, bcs::to_bytes(&voter1_address)));
        vector::push_back(&mut approvers, Vote::new_weighted_voter(4, bcs::to_bytes(&voter2_address)));
        vector::push_back(&mut approvers, Vote::new_weighted_voter(2, bcs::to_bytes(&voter3_address)));

        let (proposer, _addr, _addr_bcs) = ballot_setup(&dr);
        let proposal = TestProposal {
            test_data: 1,
        };
        let ballot_id = Vote::create_ballot(
            &proposer, // ballot_account
            *(&proposal), // proposal
            b"test_proposal", // proposal_type
            7, // num_votes_required
            approvers, // allowed_voters
            10, // expiration_timestamp_secs
        );


        // First vote does not approve the ballot
        assert!(!Vote::vote(&voter1, *(&ballot_id), b"test_proposal", *(&proposal)), 0);
        // Second vote approves the ballot
        assert!(Vote::vote(&voter2, *(&ballot_id), b"test_proposal", *(&proposal)), 0);
    }

    #[test(dr = @CoreResources)]
    #[expected_failure(abort_code = 263, location = Vote)]
    fun vote_expired_ts(dr: signer) {
        let (voter1, _voter2, _voter3, ballot_id, proposal) = vote_test_helper(&dr, 0);
        // Ballot has expired
        Vote::vote(&voter1, *(&ballot_id), b"test_proposal", *(&proposal));
    }

    #[test(dr = @CoreResources)]
    #[expected_failure(abort_code = 2049, location = Vote)]
    fun vote_repeat(dr: signer) {
        let (voter1, _voter2, _voter3, ballot_id, proposal) = vote_test_helper(&dr, 10);
        // First vote does not approve the ballot
        assert!(!Vote::vote(&voter1, *(&ballot_id), b"test_proposal", *(&proposal)), 0);
        // Cannot vote again
        Vote::vote(&voter1, *(&ballot_id), b"test_proposal", *(&proposal));
    }

    #[test(dr = @CoreResources)]
    #[expected_failure(abort_code = 1031, location = Vote)]
    fun vote_invalid_proposal_type(dr: signer) {
        let (voter1, _voter2, _voter3, ballot_id, proposal) = vote_test_helper(&dr, 10);
        // Invalid proposal type
        Vote::vote(&voter1, *(&ballot_id), b"invalid", *(&proposal));
    }

    #[test(dr = @CoreResources)]
    #[expected_failure(abort_code = 1031, location = Vote)]
    fun vote_invalid_proposal(dr: signer) {
        let (voter1, _voter2, _voter3, ballot_id, _proposal) = vote_test_helper(&dr, 10);
        let invalid_proposal = TestProposal {
            test_data: 100,
        };
        // Invalid proposal
        Vote::vote(&voter1, *(&ballot_id), b"test_proposal", invalid_proposal);
    }

    #[test(dr = @CoreResources)]
    #[expected_failure(abort_code = 769, location = Vote)]
    fun vote_invalid_ballotid(dr: signer) {
        let proposer = get_proposer();
        let (voter1, _voter2, _voter3, _ballot_id, proposal) = vote_test_helper(&dr, 10);
        let invalid_ballotid = Vote::new_ballot_id(100, signer::address_of(&proposer));
        // Invalid ballotid
        Vote::vote(&voter1, invalid_ballotid, b"test_proposal", proposal);
    }

    #[test(dr = @CoreResources)]
    #[expected_failure(abort_code = 1281, location = Vote)]
    fun vote_invalid_voter(dr: signer) {
        let (_voter1, _voter2, _voter3, ballot_id, proposal) = vote_test_helper(&dr, 10);
        let invalid_voter = vector::pop_back(&mut unit_test::create_signers_for_testing(4));
        Vote::vote(&invalid_voter, ballot_id, b"test_proposal", proposal);
    }

}
