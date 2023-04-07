/// The Vote module is used to allow voting on proposals on the chain.
/// It is typically not going to be used directly, but is intended to be
/// used as a library for modules which want to perform operations which
/// require m-of-n approvals from accounts on chain.
/// A typical workflow would look like the following
/// * Module M creates a ballot with a given `Proposal` and an approval policy using `create_ballot`
/// * It receives the BallotID corresponding to the ballot
/// * It submits votes using the `vote` function from the voters
/// * If a vote causes a ballot to be approved, `vote` returns `true` and Module M can proceed with the operation requested by the `Proposal`
module ExperimentalFramework::Vote {

    use std::bcs;
    use std::errors;
    use std::event;
    use std::signer;
    use std::vector;
    use CoreFramework::DiemTimestamp;
    #[test_only]
    friend ExperimentalFramework::VoteTests;

    /// An unique identifier for a ballot. A counter is stored
    /// under each proposers address which is incremented
    /// every time a new ballot is created. The proposers
    /// address is also part of the ballot id.
    struct BallotID has store, copy, drop {
        counter: u64,
        proposer: address,
    }

    /// WeightedVoter represents a voter with a weight
    /// The voter is represented by the bcs serialization of address
    struct WeightedVoter has store, copy, drop {
        weight: u64,
        voter: vector<u8>,
    }

    /// Ballot is a struct which contains a Proposal on which
    /// votes are gathered. A ballot is started by a proposer
    /// and it carries the proposal, the approval policy,
    /// expiration timestamp
    struct Ballot<Proposal: store + drop> has store, copy, drop {
        /// Details for the proposal being voted on.
        proposal: Proposal,
        /// A human readable type for the proposal - ex: "create_validator_owner",
        /// "freeze_account", "create_vasp", etc.
        /// This lives outside the `proposal: Proposal` to make it easy for
        /// indexers to index the ballots which have been proposed and
        /// categorize them into "types"
        proposal_type: vector<u8>,
        /// The num_votes_required for this proposal to be approved
        num_votes_required: u64,
        /// A vector of addresses which are allowed to vote on this ballot.
        allowed_voters: vector<WeightedVoter>,
        /// Votes received so far
        votes_received: vector<WeightedVoter>,
        /// Total number of weighted votes received
        total_weighted_votes_received: u64,
        // A globally unique ballot id that is created for every proposal
        ballot_id: BallotID,
        // Votes rejected after this time
        expiration_timestamp_secs: u64,
    }

    /// Ballots stores a list of ballots under a proposers address.
    /// It is type parametrized by the Proposal. Some of these may
    /// be expired and can be cleaned up using the `gc_ballots` function
    /// It also contains the handles for all the events associated
    /// with the ballots created by a proposer
    struct Ballots<Proposal: store + drop> has key {
        ballots: vector<Ballot<Proposal>>,
        create_ballot_handle: event::EventHandle<CreateBallotEvent<Proposal>>,
        remove_ballot_handle: event::EventHandle<RemoveBallotEvent>,
        voted_handle: event::EventHandle<VotedEvent>,
        ballot_approved_handle: event::EventHandle<BallotApprovedEvent>,
    }

    /// A counter which is stored under the proposers address and gets incremented
    /// everytime they create a new ballot. This is used for creating unique
    /// global identifiers for Ballots
    struct BallotCounter has key {
        counter: u64,
    }

    /// CreateBallotEvent is emitted when a ballot is
    /// created by a proposer
    struct CreateBallotEvent<Proposal: store + drop> has drop, store {
        ballot_id: BallotID,
        ballot: Ballot<Proposal>,
    }

    /// RemoveBallotEvent is emitted when a ballot has
    /// been removed. This can either happen because:
    /// * ballot was approved
    /// * ballot was manually removed by the proposer
    /// * ballot was expired and garbage collected
    struct RemoveBallotEvent has drop, store {
        ballot_id: BallotID,
    }

    /// VotedEvent is emitted when a valid vote has
    /// been accepted by the ballot
    struct VotedEvent has drop, store {
        ballot_id: BallotID,
        voter: address,
        vote_weight: u64,
    }

    /// BallotApprovedEvent is emitted when a ballot has
    /// been approved by the voters
    struct BallotApprovedEvent has drop, store {
        ballot_id: BallotID,
    }

    /// The maximum number of ballots allowed per proposal type
    /// per address.
    const MAX_BALLOTS_PER_PROPOSAL_TYPE_PER_ADDRESS: u64 = 256;

    /// The provided timestamp(s) were invalid
    const EINVALID_TIMESTAMP: u64 = 1;
    /// The address already contains has the maximum of ballots allowed
    /// MAX_BALLOTS_PER_PROPOSAL_TYPE_PER_ADDRESS
    const ETOO_MANY_BALLOTS: u64 = 2;
    /// Ballot with the provided id was not found
    const EBALLOT_NOT_FOUND: u64 = 3;
    /// Proposal details in the vote do not match the proposal details
    /// in the ballot
    const EBALLOT_PROPOSAL_MISMATCH: u64 = 4;
    /// Voter not allowed to vote in the ballot
    const EINVALID_VOTER: u64 = 5;
    /// current timestamp > ballot expiration time
    const EBALLOT_EXPIRED: u64 = 7;
    /// Voter has already voted in this ballot
    const EALREADY_VOTED: u64 = 8;
    /// Num_votes must be greater than 0, so that election is not won when started
    const EINVALID_NUM_VOTES: u64 = 9;

    /// A constructor for BallotID
    public fun new_ballot_id(
        counter: u64,
        proposer: address,
    ): BallotID {
        BallotID {
            counter,
            proposer,
        }
    }

    /// A constructor for WeightedVoter
    public fun new_weighted_voter(
        weight: u64,
        voter: vector<u8>,
    ): WeightedVoter {
        WeightedVoter {
            weight,
            voter,
        }
    }

    /// Create a ballot under the signer's address and return the `BallotID`
    public fun create_ballot<Proposal: store + copy + drop>(
        ballot_account: &signer,
        proposal: Proposal,
        proposal_type: vector<u8>,
        num_votes_required: u64,
        allowed_voters: vector<WeightedVoter>,
        expiration_timestamp_secs: u64
    ): BallotID acquires Ballots, BallotCounter {
        let ballot_address = signer::address_of(ballot_account);

        assert!(DiemTimestamp::now_seconds() < expiration_timestamp_secs, errors::invalid_argument(EINVALID_TIMESTAMP));
        assert!(num_votes_required > 0, errors::invalid_argument(EINVALID_NUM_VOTES));

        if (!exists<BallotCounter>(ballot_address)) {
            move_to(ballot_account, BallotCounter {
                counter: 0,
            });
        };
        if (!exists<Ballots<Proposal>>(ballot_address)) {
            move_to(ballot_account, Ballots<Proposal> {
                ballots: vector::empty(),
                create_ballot_handle: event::new_event_handle<CreateBallotEvent<Proposal>>(ballot_account),
                remove_ballot_handle: event::new_event_handle<RemoveBallotEvent>(ballot_account),
                voted_handle: event::new_event_handle<VotedEvent>(ballot_account),
                ballot_approved_handle: event::new_event_handle<BallotApprovedEvent>(ballot_account),
            });
        };

        let ballot_data = borrow_global_mut<Ballots<Proposal>>(ballot_address);

        // Remove any expired ballots
        gc_internal<Proposal>(ballot_data);
        let ballots = &mut ballot_data.ballots;

        assert!(vector::length(ballots) < MAX_BALLOTS_PER_PROPOSAL_TYPE_PER_ADDRESS, errors::limit_exceeded(ETOO_MANY_BALLOTS));
        let ballot_id = new_ballot_id(incr_counter(ballot_account), ballot_address);
        let ballot = Ballot<Proposal> {
            proposal,
            proposal_type,
            num_votes_required,
            allowed_voters,
            votes_received: vector::empty(),
            total_weighted_votes_received: 0,
            ballot_id: *&ballot_id,
            expiration_timestamp_secs,
        };
        vector::push_back(ballots, *&ballot);
        event::emit_event<CreateBallotEvent<Proposal>>(
            &mut ballot_data.create_ballot_handle,
            CreateBallotEvent {
                ballot_id: *&ballot_id,
                ballot,
            },
        );
        ballot_id
    }

    // Checks if a voter is present in the vector<WeightedVoter>
    fun check_voter_present(
        weighted_voters: &vector<WeightedVoter>,
        voter: &vector<u8>,
    ): bool {
        let i = 0;
        let len = vector::length(weighted_voters);
        while (i < len) {
            if (&vector::borrow(weighted_voters, i).voter == voter) return true;
            i = i + 1;
        };
        false
    }

    /// Submit a vote from the `voter_account` to `ballot_id`
    /// This also contains the `proposal_type` and `proposal` so that
    /// the voter signs over these when sending their vote.
    /// If this vote causes the ballot to be approved, then the ballot
    /// is removed from the proposers address.
    /// This returns a bool indicating whether this vote moved the ballot
    /// to an approved status. true represents that the ballot got approved
    /// after this vote and false represents that the ballot has not been
    /// approved after this vote
    public fun vote<Proposal: store + drop>(
        voter_account: &signer,
        ballot_id: BallotID,
        proposal_type: vector<u8>,
        proposal: Proposal,
    ): bool acquires Ballots {
        let ballot_data = borrow_global_mut<Ballots<Proposal>>(ballot_id.proposer);

        // Remove any expired ballots
        gc_internal<Proposal>(ballot_data);

        let ballots = &mut ballot_data.ballots;
        let i = 0;
        let len = vector::length(ballots);
        while (i < len) {
            if (&vector::borrow(ballots, i).ballot_id == &ballot_id) break;
            i = i + 1;
        };
        assert!(i < len, errors::invalid_state(EBALLOT_NOT_FOUND));
        let ballot_index = i;
        let ballot = vector::borrow_mut(ballots, ballot_index);

        assert!(&ballot.proposal == &proposal, errors::invalid_argument(EBALLOT_PROPOSAL_MISMATCH));
        assert!(&ballot.proposal_type == &proposal_type, errors::invalid_argument(EBALLOT_PROPOSAL_MISMATCH));

        let voter_address = signer::address_of(voter_account);
        let voter_address_bcs = bcs::to_bytes(&voter_address);
        let allowed_voters = &ballot.allowed_voters;

        assert!(check_voter_present(allowed_voters, &voter_address_bcs), errors::invalid_state(EINVALID_VOTER));
        assert!(DiemTimestamp::now_seconds() <= ballot.expiration_timestamp_secs, errors::invalid_state(EBALLOT_EXPIRED));

        assert!(!check_voter_present(&ballot.votes_received, &voter_address_bcs), errors::invalid_state(EALREADY_VOTED));

        let i = 0;
        let len = vector::length(allowed_voters);
        while (i < len) {
            let weighted_voter = vector::borrow(allowed_voters, i);
            if (&weighted_voter.voter == &voter_address_bcs) {
                vector::push_back(&mut ballot.votes_received, *weighted_voter);
                ballot.total_weighted_votes_received = ballot.total_weighted_votes_received + weighted_voter.weight;
                event::emit_event<VotedEvent>(
                    &mut ballot_data.voted_handle,
                    VotedEvent {
                        ballot_id: *&ballot_id,
                        voter: voter_address,
                        vote_weight: weighted_voter.weight,
                    },
                );
                break
            };
            i = i + 1;
        };
        let ballot_approved = ballot.total_weighted_votes_received >= ballot.num_votes_required;
        // If the ballot gets approved, remove the ballot immediately
        if (ballot_approved) {
            vector::swap_remove(ballots, ballot_index);
            event::emit_event<BallotApprovedEvent>(
                &mut ballot_data.ballot_approved_handle,
                BallotApprovedEvent {
                    ballot_id,
                },
            );
        };
        ballot_approved
    }

    /// gc_ballots deletes all the expired ballots of the type `Proposal`
    /// under the provided address `addr`. The signer can be anybody
    /// and does not need to have the same address as `addr`
    public entry fun gc_ballots<Proposal: store + drop>(
        _signer: signer,
        addr: address,
    ) acquires Ballots {
        gc_internal<Proposal>(borrow_global_mut<Ballots<Proposal>>(addr));
    }

    public(friend) fun gc_test_helper<Proposal: store + drop>(
        addr: address,
    ): vector<BallotID>  acquires Ballots {
        gc_internal<Proposal>(borrow_global_mut<Ballots<Proposal>>(addr))
    }

    fun gc_internal<Proposal: store + drop>(
        ballot_data: &mut Ballots<Proposal>,
    ): vector<BallotID> {
        let ballots = &mut ballot_data.ballots;
        let remove_handle = &mut ballot_data.remove_ballot_handle;
        let i = 0;
        let removed_ballots = vector::empty();
        while ({
            spec {
                invariant unique_ballots(ballots);
                invariant no_expired_ballots(ballots, DiemTimestamp::spec_now_seconds(), i);
                invariant vector_subset(ballots, old(ballot_data).ballots);
                invariant i <= len(ballots);
                invariant 0 <= i;
            };
            i < vector::length(ballots)
        }) {
            let ballot = vector::borrow(ballots, i);
            if (ballot.expiration_timestamp_secs < DiemTimestamp::now_seconds()) {
                let ballot_id = *(&ballot.ballot_id);
                vector::swap_remove(ballots, i);
                vector::push_back(&mut removed_ballots, *&ballot_id);
                event::emit_event<RemoveBallotEvent>(
                    remove_handle,
                    RemoveBallotEvent {
                        ballot_id
                    },
                );
            } else {
                i = i + 1;
            };
        };
        removed_ballots
    }

    public(friend) fun remove_ballot_internal<Proposal: store + drop>(
        account: signer,
        ballot_id: BallotID,
    ) acquires Ballots {
        let addr = signer::address_of(&account);
        let ballot_data = borrow_global_mut<Ballots<Proposal>>(addr);
        let ballots = &mut ballot_data.ballots;
        let remove_handle = &mut ballot_data.remove_ballot_handle;
        let i = 0;
        let len = vector::length(ballots);
        while ({
            spec { invariant ballot_id_does_not_exist<Proposal>(ballot_id, ballots, i); };
            i < len
        }) {
            if (&vector::borrow(ballots, i).ballot_id == &ballot_id) {
                vector::swap_remove(ballots, i);
                event::emit_event<RemoveBallotEvent>(
                    remove_handle,
                    RemoveBallotEvent {
                        ballot_id
                    },
                );
                return ()
            };
            i = i + 1;
        };
    }

    /// remove_ballot removes the ballot with the provided
    /// `ballot_id` from the address of the provided `account`
    /// If a ballot with `ballot_id` is not found, then it
    /// does nothing
    public fun remove_ballot<Proposal: store + drop>(
        account: signer,
        ballot_id: BallotID,
    ) acquires Ballots {
        remove_ballot_internal<Proposal>(account, ballot_id)
    }

    /// incr_counter increments the counter stored under the signer's
    /// account
    fun incr_counter(account: &signer): u64 acquires BallotCounter {
        let addr = signer::address_of(account);
        let counter = &mut borrow_global_mut<BallotCounter>(addr).counter;
        let count = *counter;
        *counter = *counter + 1;
        count
    }

    ///****************************************************************
    /// Specs
    ///****************************************************************

    /// I (DD) was experimenting with some new ideas about top-down specification.
    /// This is a partial specification, but it does have some interesting properties
    /// and is good for testing the Prover.

    /// A "Ballot" keeps track of an election for a "Proposal" type at a particular address.

    /// To conduct an election at a particular address, there must be a BallotCounter
    /// published at that address.  This keeps track of a counter that is used to generate
    /// unique BallotIDs.

    spec module {
        /// Once the BallotCounter is published, it remains published forever
        invariant update forall ballot_addr: address where old(exists<BallotCounter>(ballot_addr)):
            exists<BallotCounter>(ballot_addr);

         /// Once a proposal is initialized, it stays initialized forever.
         invariant<Proposal> update forall ballot_addr: address
             where old(exists<Ballots<Proposal>>(ballot_addr)):
                 exists<Ballots<Proposal>>(ballot_addr);
    }

    /// Predicate to test if a `Ballots` resource for `Proposal` is published at `ballot_addr`,
    /// there is a `BallotCounter` published at `ballot_addr`.
    spec fun ballot_counter_initialized_first<Proposal>(ballot_addr: address): bool {
        exists<Ballots<Proposal>>(ballot_addr) ==> exists<BallotCounter>(ballot_addr)
    }

    spec module {
        // / Whenever there is a Ballots<Proposal> at ballot_address, there is
        // / a BallotCounter there.
        // TODO: Because of prover bug, this is temporarily ANDed with another invariant.
        // invariant<Proposal> forall ballot_addr: address:
        //     ballot_counter_initialized_first<Proposal>(ballot_addr);
    }

    // UTILITY FUNCTIONS

    /// Get the ballots vector from published Ballots<Proposal>
    /// CAUTION: Returns an arbitrary value if no Ballots<Proposal> is publised at ballot_address.
    spec fun get_ballots<Proposal>(ballot_address: address): vector<Ballot<Proposal>> {
       global<Ballots<Proposal>>(ballot_address).ballots
    }

    /// Get the ballot matching ballot_id out of the ballots vector, if it is there.
    /// CAUTION: Returns a arbitrary value if it's not there.
    spec fun get_ballot<Proposal>(ballot_address: address, ballot_id: BallotID): Ballot<Proposal> {
         let ballots = global<Ballots<Proposal>>(ballot_address).ballots;
         get_ballots<Proposal>(ballot_address)[choose min i in 0..len(ballots) where ballots[i].ballot_id == ballot_id]
     }

    /// Tests whether ballot_id is represented in the ballots vector. Returns false if there is no
    /// ballots vector.
    spec fun ballot_exists<Proposal>(ballot_address: address, ballot_id: BallotID): bool {
        if (exists<Ballots<Proposal>>(ballot_address)) {
            let ballots = global<Ballots<Proposal>>(ballot_address).ballots;
            exists i in 0..len(ballots): ballots[i].ballot_id == ballot_id
        }
        else
            false
    }

    /// Assuming ballot exists, check if it's expired. Returns an arbitrary result if the
    /// ballot does not exist.
    /// NOTE: Maybe this should be "<=" not "<"
    spec fun is_expired_if_exists<Proposal>(ballot_address: address, ballot_id: BallotID): bool {
        get_ballot<Proposal>(ballot_address, ballot_id).expiration_timestamp_secs
            <= DiemTimestamp::spec_now_seconds()
    }

    // FUNCTIONS REPRESENTING STATES

    /// There is a state machine for every `BallotID<Proposal>`.  Two of the states don't
    /// need to appear in formal specifications, but they are here for completeness.
    /// State: unborn -- The `BallotID` has a count that is greater than the count in BallotCounter.
    ///   So the `BallotID` has not yet been generated, and may be generated in the future.
    ///   It is not in use, so we won't see values in this state.
    ///   A `BallotID` in the unborn state may transition to the active state if `create_ballot`
    ///     generates that `BallotID`.
    /// State: dead -- The `BallotID` was generated and then was either and accepted (when
    ///   a call to `vote` causes the vote total to exceed the threshold),
    ///   or expired and garbage collected.  So, it is no longer in use and we won't see these
    ///   values.  Note that garbage collection occurs in several functions.
    /// State active -- The `BallotID` is in a `Ballots<Proposal>.ballots` vector for some Proposal and
    ///    address.  It is not expired and may eventually be accepted.
    ///    Active BallotIDs are created and returned by `create_ballot`.
    ///    An active `BallotID` may transition to the expired state if it is not accepted and the current
    ///    time exceeds its expiration time, or it may transition to the dead state if it expires and is
    ///    garbage-collected in the same transaction or if it is accepted.
    /// State expired -- The `BallotID` is in a `Ballots<Proposal>.ballots` vector for some Proposal and
    ///    address but is expired.  It will be removed from the ballots vector and change to the dead state
    ///    if and when it is garbage-collected.

    /// A BallotID is in the expired state if it is in the ballots vector and the
    /// current time is >= the expiration time.
    spec fun is_expired<Proposal>(ballot_address: address, ballot_id: BallotID): bool {
        ballot_exists<Proposal>(ballot_address, ballot_id)
        && is_expired_if_exists<Proposal>(ballot_address, ballot_id)
    }

    /// A BallotID is active state if it is in the ballots vector and not expired.
    spec fun is_active<Proposal>(ballot_address: address, ballot_id: BallotID): bool {
       ballot_exists<Proposal>(ballot_address, ballot_id)
       && !is_expired_if_exists<Proposal>(ballot_address, ballot_id)
    }

    spec create_ballot {
        /// create_ballot sets up a `Ballots<Proposal>` resource at the `ballot_account`
        /// address if one does not already exist.
        ensures exists<Ballots<Proposal>>(signer::address_of(ballot_account));

        /// returns a new active `BallotID`.
        ensures is_active<Proposal>(signer::address_of(ballot_account), result);
    }

    /// Returns "true" iff there are no ballots in v at indices less than i whose
    /// expiration time is less than or equal to the current time.
    spec fun no_expired_ballots<Proposal>(ballots: vector<Ballot<Proposal>>, now_seconds: u64, i: u64): bool {
        forall j in 0..i: ballots[j].expiration_timestamp_secs >= now_seconds
    }

    // This is equivalent to mapping each ballot in v to its ballot_id.
    // TODO: A map operation in the spec language would be much clearer.
    spec fun extract_ballot_ids<Proposal>(v: vector<Ballot<Proposal>>): vector<BallotID> {
        choose result: vector<BallotID> where len(result) == len(v)
        && (forall i in 0..len(v): result[i] == v[i].ballot_id)
    }

    /// Common post-conditions for `gc_internal` and `gc_ballots` (which just calls `gc_internal`)
    spec schema GcEnsures<Proposal> {
        ballot_data: Ballots<Proposal>;
        let pre_ballots = ballot_data.ballots;
        let post post_ballots = ballot_data.ballots;

        /// Ballots afterwards is a subset of ballots before.
        ensures vector_subset(post_ballots, pre_ballots);
        /// All expired ballots are removed
        ensures no_expired_ballots<Proposal>(post_ballots, DiemTimestamp::spec_now_seconds(), len(post_ballots));
    }

    spec gc_internal {
        pragma opaque;
        include GcEnsures<Proposal>;
        // Note: There is no specification of returned vector of removed ballot ids, because
        // return value seems not to be used.
    }

    spec gc_ballots {
        include GcEnsures<Proposal>{ballot_data: global<Ballots<Proposal>>(addr)};
    }

    // Lower-level invariants

    // helper functions

    spec fun ballot_ids_have_correct_ballot_address<Proposal>(proposer_address: address): bool {
       let ballots = get_ballots<Proposal>(proposer_address);
       forall i in 0..len(ballots): ballots[i].ballot_id.proposer == proposer_address
    }

    /// Every ballot for Proposal at proposer_address has a ballot counter field that is less
    /// than the current value of the BallotCounter.counter published at proposer_address.
    /// This property is necessary to show that the ballot IDs are not repeated in the
    /// Ballots.ballots vector
    spec fun existing_ballots_have_small_counters<Proposal>(proposer_address: address): bool {
        // Just return true if there is no Ballots<Proposal> published at proposer_address
        // get_ballots may be undefined here, but we only use it when we know the Ballots
        // is published (in the next property.
        let ballots = get_ballots<Proposal>(proposer_address);
        exists<Ballots<Proposal>>(proposer_address)
        ==> (forall i in 0..len(ballots):
                ballots[i].ballot_id.counter < global<BallotCounter>(proposer_address).counter)
    }

     /// Every ballot in Ballots<Proposal>.ballots is active or expired.
     /// I.e., none have sum >= required.
     /// TODO: This should be part of is_active/expired, and should follow from an invariant
     /// that every BallotID is in one of the legal states.
     spec fun no_winning_ballots_in_vector<Proposal>(proposer_address: address): bool {
         let ballots = get_ballots<Proposal>(proposer_address);
         forall i in 0..len(ballots):
             ballots[i].total_weighted_votes_received < ballots[i].num_votes_required
     }

    spec module {
        /// ballots in vector all have the proposer address in their ballot IDs.
        invariant<Proposal> [suspendable] forall proposer_address: address:
            ballot_ids_have_correct_ballot_address<Proposal>(proposer_address);

        // / counter values in ballots are all less than the value of the BallotCounter
        // / See note on spec fun existing_ballots_have_small_counters
        // TODO: Temporarily commented out because of a prover bug. It is included in
        // the next property
        // invariant<Proposal> forall addr: address: existing_ballots_have_small_counters<Proposal>(addr);

        // AND of these two invariants works, but they don't if individual due to a bug.
        invariant<Proposal>
            (forall addr: address: existing_ballots_have_small_counters<Proposal>(addr))
            && (forall ballot_addr: address: ballot_counter_initialized_first<Proposal>(ballot_addr));

        /// Every ballot in the vector has total_weighted_votes_received < num_votes_required
        /// So the ballot will eventually be removed either by accumulating enough votes or by expiring
        /// and being garbage-collected
        invariant<Proposal> forall addr: address: no_winning_ballots_in_vector<Proposal>(addr);
    }

    /// There are no duplicate Ballot IDs in the Ballots<Proposer>.ballots vector
    spec fun unique_ballots<Proposal>(ballots: vector<Ballot<Proposal>>): bool {
        forall i in 0..len(ballots), j in 0..len(ballots):
            ballots[i].ballot_id == ballots[j].ballot_id ==> i == j
    }

    /// All `BallotID`s of `Ballot`s in a `Ballots.ballots` vector are unique.
    spec Ballots {
        invariant unique_ballots(ballots);
    }

    /// Asserts that ballot ID is not in ballots vector.  Used in loop invariant
    /// and post-condition of remove_ballot_internal
    spec fun ballot_id_does_not_exist<Proposal>(ballot_id: BallotID, ballots: vector<Ballot<Proposal>>, i: u64): bool {
        forall j in 0..i: ballots[j].ballot_id != ballot_id
    }

    spec remove_ballot_internal {
        let post ballots = get_ballots<Proposal>(signer::address_of(account));
        ensures
            ballot_id_does_not_exist<Proposal>(ballot_id, ballots, len(ballots));
    }

    spec remove_ballot {
       let post ballots = get_ballots<Proposal>(signer::address_of(account));
       ensures
           ballot_id_does_not_exist<Proposal>(ballot_id, ballots, len(ballots));
    }

    spec gc_test_helper {
        // Just a test function, we don't need to spec it.
        pragma verify = false;
    }

    // helper functions
    spec fun vector_subset<Elt>(v1: vector<Elt>, v2: vector<Elt>): bool {
        forall e in v1: exists i in 0..len(v2): v2[i] == e
    }
}
