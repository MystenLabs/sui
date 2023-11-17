// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Simple upgrade policy that requires a `k` out of `n` quorum in order to perform
/// a proposed upgrade.
/// 
/// This policy is initiated with a call to `k_of_n_upgrade_policy::new` providing 
/// the `UpgradeCap` of the package to be controlled by the policy, the `k` value 
/// (number of votes to be received for the upgrade to be allowed) and the list of
/// `address`es allowed to vote. The `address`es provided will receive
/// a `Ballot` for any given proposal. The sender of the transaction that creates
/// the `KofNUpgradeCap` is added to the list of voters as well.
/// The set of possible voters is provided at creation time and it is immutable for
/// the lifetime of the policy.
/// This policy allows for a number of voters between 2 and 100.
/// 
/// An upgrade is proposed via `k_of_n_upgrade_policy::propose_upgrade` and the sender
/// of the upgrade not only needs to have a reference to the `KofNUpgradeCap` but they 
/// must also be in the list of possible voters.
/// A successful call for a valid proposal will result in all `address`es
/// registered to receive a `Ballot` which would allow them to vote for the proposal.
/// The proposer will not receive a `Ballot`.
/// Receiver of the `Ballot` can then vote for the upgrade. The `Ballot` can also
/// be sent to other entities which can then vote for the upgrade on behalf of
/// the original owner.
/// 
/// Once the number of votes is reached the proposer of the upgrade can perform
/// the upgrade.
/// 
/// Events are emitted to track the main operations on the proposal.
/// A proposed upgrade lifetime is tracked via the 4 events:
/// `UpgradeProposed`, `UpgradeVoted` and `UpgradePerformed` or `UpgradeDiscarded`.
/// Also `Ballot`s are tracked via `BallotTransfered` and `BallotDeleted` in order to
/// audit the lifetime of a vote.
/// 
/// Multiple upgrades can be live at the same time. That is not the expected behavior
/// but there are no restriction to the number of upgrades open at any point in time.
/// When that happens the first upgrade executed "wins" and subsequent attempt to
/// authorize an upgrade will fail as the version will not match any longer.
module sui::k_of_n_upgrade_policy {
    use std::vector;
    use sui::event;
    use sui::object::{Self, ID, UID};
    use sui::package::{Self, UpgradeCap, UpgradeTicket, UpgradeReceipt};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::vec_set::{Self, VecSet};

    /// The capability controlling the upgrade. 
    /// Initialized with `new` is returned to the caller to be stored as desired.
    struct KofNUpgradeCap has key, store {
        id: UID,
        /// Upgrade cap of the package controlled by this policy.
        cap: UpgradeCap,
        /// Number of votes required for the upgrade to be allowed.
        required_votes: u64,
        /// Allowed voters. They will receive a ballot. They can be addresses or objects.
        /// The creator of the policy will be added to the voters if not there already.
        voters: VecSet<address>,
    }

    /// A proposed upgrade that is going through voting.
    /// `ProposedUpgrade` instances are shared objects that will be passed as 
    /// an argument, together with a `Ballot`, when voting.
    /// It's possible to have multiple proposed upgrades at the same time and 
    /// the first successful update will obsolete all the others, given
    /// an attempt to upgrade with a "concurrent" one will fail because of
    /// versioning.
    struct ProposedUpgrade has key {
        id: UID,
        /// The ID of the `KofNUpgradeCap` that this vote was initiated from.
        cap: ID,
        /// The address requesting permission to perform the upgrade.
        /// This is the sender of the transaction that proposes and 
        /// performs the upgrade.
        signer: address,
        /// The digest of the bytecode that the package will be upgraded to.
        digest: vector<u8>,
        /// The ballots allowed to vote for this upgrade.
        allowed_voters: VecSet<ID>,
        /// The current voters that have accepted the upgrade.
        current_voters: VecSet<ID>,
    }

    /// A request to vote for an upgrade. Each possible voter will receive a `Ballot`
    /// that they use to vote for the upgrade. 
    struct Ballot has key {
        id: UID,
        /// The original address the ballot was sent to.
        owner: address,
        /// The count of transfers this ballot went through. 
        /// It is informational only and can be used to track transfers of
        /// `Ballot`'s instances.
        transfers_count: u64,
        /// The digest of the bytecode that the package will be upgraded to.
        digest: vector<u8>,
        /// The ID of the `ProposedUpgrade` this Ballot refers to.
        proposed_upgrade: ID,
    }

    // 
    // Events to track history and progress of upgrades
    //

    /// A new proposal for an upgrade.
    struct UpgradeProposed has copy, drop {
        /// the instance of the k out of n policy
        upgrade_cap: ID,
        /// the ID of the proposal (`ProposedUpgrade` instance)
        proposal: ID,
        /// digest of the proposal
        digest: vector<u8>,
        /// the address (sender) of the proposal
        proposer: address,
        /// allowed voters
        voters: VecSet<address>,
        /// ballots sent out
        ballots: VecSet<ID>,
    }

    /// A vote for a given upgrade.
    struct UpgradeVoted has copy, drop {
        /// the ID of the proposal (`ProposedUpgrade` instance)
        proposal: ID,
        /// digest of the proposal
        digest: vector<u8>,
        /// ballot used for this vote
        ballot: ID,
        /// sender of the vote transaction
        voter: address,
    }

    /// A succesful upgrade.
    struct UpgradePerformed has copy, drop {
        /// the instance of the k out of n policy
        upgrade_cap: ID,
        /// the ID of the proposal (`ProposedUpgrade` instance)
        proposal: ID,
        /// digest of the proposal
        digest: vector<u8>,
        /// signer of the upgrade
        signer: address,
    }

    /// A discarded upgrade.
    struct UpgradeDiscarded has copy, drop {
        /// the instance of the k out of n policy
        upgrade_cap: ID,
        /// the ID of the proposal (`ProposedUpgrade` instance)
        proposal: ID,
        /// digest of the proposal
        digest: vector<u8>,
        /// signer of the discarded upgrade transaction
        signer: address,
    }

    /// Record the transfer for a `Ballot`.
    struct BallotTransfered has copy, drop {
        /// the ID of the proposal (`ProposedUpgrade` instance)
        proposal: ID,
        /// digest of the proposal
        digest: vector<u8>,
        /// ballot used for this vote
        ballot: ID,
        /// signer of the transfer operation
        signer: address,
        /// original owner of the ballot, that is the address the
        /// ballot was first sent to
        original_owner: address,
        /// the receiver of the ballot
        ballot_receiver: address,
    }

    /// Record a deleted `Ballot`.
    struct BallotDeleted has copy, drop {
        /// the ID of the proposal (`ProposedUpgrade` instance)
        proposal: ID,
        /// digest of the proposal
        digest: vector<u8>,
        /// ballot used for this vote
        ballot: ID,
        /// signer of the deleted operation
        signer: address,
        /// original owner of the ballot, that is the address the
        /// ballot was first sent to
        original_owner: address,
    }

    /// Allowed voters must in the [2, 100] range.
    const EAllowedVotersError: u64 = 0;
    /// Required votes must be less than allowed voters.
    const ERequiredVotesError: u64 = 1;
    /// The `Ballot` used to vote is not for the correct proposal (`ProposedUpgrade`)
    const EInvalidBallot: u64 = 2;
    /// Not enough votes to perform the upgrade.
    const ENotEnoughVotes: u64 = 3;
    /// An upgrade was issued already, and the operation requested failed.
    const EAlreadyIssued: u64 = 4;
    /// The operation required the signer to be the same as the upgrade proposer.
    const ESignerMismatch: u64 = 5;
    /// Proposal (`KofNUpgradeCap`) and upgrade (`ProposedUpgrade`) do not match.
    const EInvalidProposalForUpgrade: u64 = 6;

    /// Create a `KofNUpgradeCap` given an `UpgradeCap`.
    /// The returned instance is the only and exclusive controller of upgrades. 
    /// If the transaction sender is not in the set of allowed voters it will be
    /// added. The sender is the publisher of the package which is always allowed
    /// to vote.
    /// The `k` (`required_votes`) out of `n` (length of `voters`) is set up
    /// at construction time and it is immutable.
    public fun new(
        cap: UpgradeCap,
        required_votes: u64,
        voters: VecSet<address>,
        ctx: &mut TxContext,
    ): KofNUpgradeCap {
        assert!(vec_set::size(&voters) > 1, EAllowedVotersError);
        assert!(vec_set::size(&voters) <= 100, EAllowedVotersError);
        assert!(required_votes <= vec_set::size(&voters), ERequiredVotesError);
        KofNUpgradeCap {
            id: object::new(ctx),
            cap,
            required_votes,
            voters,
        }
    }

    /// Make the package immutable by destroying the k of n upgrade cap and the
    /// underlying upgrade cap.
    public fun make_immutable(upgrade_cap: KofNUpgradeCap) {
        let KofNUpgradeCap {
            id,
            cap,
            required_votes: _,
            voters: _,
        } = upgrade_cap;
        object::delete(id);
        package::make_immutable(cap);
    }

    /// Restrict upgrades to "add code only", or "change dependencies".
    public fun only_additive_upgrades(cap: &mut KofNUpgradeCap) {
        package::only_additive_upgrades(&mut cap.cap)
    }

    /// Restrict upgrades to "change dependencies only".
    public fun only_dep_upgrades(cap: &mut KofNUpgradeCap) {
        package::only_dep_upgrades(&mut cap.cap)
    }

    /// Propose an upgrade. 
    /// The `digest` of the proposed upgrade is provided to identify the upgrade.
    /// Ballots are distributed to all possible voters and a `ProposedUpgrade`
    /// is created and saved as a shared object. 
    /// The proposer is the sender of the transaction.
    public fun propose_upgrade(
        cap: &KofNUpgradeCap,
        digest: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let cap_id = object::id(cap);
        let proposal_uid = object::new(ctx);
        let proposal_id = object::uid_to_inner(&proposal_uid);
        
        // send a `Ballot` to all allowed voters
        let allowed_voters = vec_set::empty();
        let addresses = vec_set::keys(&cap.voters);
        let ballot_count = vector::length(addresses);
        while (ballot_count > 0) {
            ballot_count = ballot_count - 1;
            let address = *vector::borrow(addresses, ballot_count);
            let ballot_uid = object::new(ctx);
            let ballot_id = object::uid_to_inner(&ballot_uid);
            transfer::transfer(
                Ballot {
                    id: ballot_uid,
                    owner: address,
                    transfers_count: 0,
                    digest,
                    proposed_upgrade: proposal_id,
                },
                address,
            );
            vec_set::insert(&mut allowed_voters, ballot_id);
        };

        let signer = tx_context::sender(ctx);
        event::emit(UpgradeProposed {
            upgrade_cap: cap_id,
            proposal: proposal_id,
            digest,
            proposer: signer,
            voters: cap.voters,
            ballots: allowed_voters,
        });

        transfer::share_object(ProposedUpgrade {
            id: proposal_uid,
            cap: cap_id,
            signer,
            digest,
            allowed_voters,
            current_voters: vec_set::empty(),
        })
    }

    /// Vote in favor of an upgrade, aborts if the ballot is not for the proposed
    /// upgrade or if the upgrade was already performed.
    public fun vote(
        proposal: &mut ProposedUpgrade, 
        ballot: Ballot,
        ctx: &TxContext,
    ) {
        assert!(proposal.signer != @0x0, EAlreadyIssued);
        let Ballot { 
            id, 
            owner: _,
            transfers_count: _,
            digest,
            proposed_upgrade,
        } = ballot;
        assert!(digest == proposal.digest, EInvalidBallot);
        let proposal_id = object::id(proposal);
        assert!(proposal_id == proposed_upgrade, EInvalidBallot);
        let ballot_id = object::uid_to_inner(&id);
        vec_set::insert(&mut proposal.current_voters, ballot_id);
        object::delete(id);

        event::emit(UpgradeVoted {
            proposal: object::id(proposal),
            digest,
            ballot: ballot_id,
            voter: tx_context::sender(ctx),
        });
    }

    /// Issue an `UpgradeTicket` for the upgrade being voted on.  Aborts if 
    /// there are not enough votes yet, or if the upgrade was already performed.
    /// The signer of the transaction must be the same as the one proposing the
    /// upgrade.
    public fun authorize_upgrade(
        cap: &mut KofNUpgradeCap,
        proposal: &mut ProposedUpgrade, 
        ctx: &TxContext,
    ): UpgradeTicket {
        assert!(proposal.cap == object::id(cap), EInvalidProposalForUpgrade);
        assert!(
            vec_set::size(&proposal.current_voters) >= cap.required_votes, 
            ENotEnoughVotes,
        );

        // assert the signer is the proposer and the upgrade has not happened yet
        let signer = tx_context::sender(ctx);
        assert!(proposal.signer != @0x0, EAlreadyIssued);
        assert!(proposal.signer == signer, ESignerMismatch);
        proposal.signer = @0x0;

        event::emit(UpgradePerformed {
            upgrade_cap: proposal.cap,
            proposal: object::id(proposal),
            digest: proposal.digest,
            signer,
        });

        let policy = package::upgrade_policy(&cap.cap);
        package::authorize_upgrade(
            &mut cap.cap,
            policy,
            proposal.digest,
        )
    }

    /// Finalize the upgrade to produce the given receipt.
    public fun commit_upgrade(
        cap: &mut KofNUpgradeCap, 
        receipt: UpgradeReceipt,
    ) {
        package::commit_upgrade(&mut cap.cap, receipt)
    }

    /// Transfer a `Ballot` to the given `address`. 
    /// Record the fact a transfer was performed.
    public fun transfer(ballot: Ballot, receiver: address, ctx: &TxContext) {
        ballot.transfers_count = ballot.transfers_count + 1;
        event::emit(BallotTransfered {
            proposal: ballot.proposed_upgrade,
            digest: ballot.digest,
            ballot: object::id(&ballot),
            signer: tx_context::sender(ctx),
            original_owner: ballot.owner,
            ballot_receiver: receiver,
        });
        transfer::transfer(ballot, receiver)
    }

    /// Discard an existing proposed upgrade.
    public fun discard_proposed_upgrade(proposed_upgrade: ProposedUpgrade, ctx: &TxContext) {
        let proposal = object::id(&proposed_upgrade);
        let ProposedUpgrade {
            id,
            cap,
            signer,
            digest,
            allowed_voters: _,
            current_voters: _,
        } = proposed_upgrade;
        assert!(signer == tx_context::sender(ctx), ESignerMismatch);
        event::emit(UpgradeDiscarded {
            upgrade_cap: cap,
            proposal,
            digest,
            signer,
        });
        object::delete(id);
    }

    /// Destroy a ballot.
    public fun destroy_ballot(ballot: Ballot, ctx: &TxContext) {
        let ballot_id = object::id(&ballot);
        let Ballot { 
            id, 
            owner,
            transfers_count: _,
            digest,
            proposed_upgrade,
        } = ballot;
        event::emit(BallotDeleted {
            proposal: proposed_upgrade,
            digest,
            ballot: ballot_id,
            signer: tx_context::sender(ctx),
            original_owner: owner,
        });
        object::delete(id);
    }

    //
    // Accessors
    //

    /// Get the `UpgradeCap` of the package protected by the k out of n policy.
    public fun upgrade_cap(cap: &KofNUpgradeCap): &UpgradeCap {
        &cap.cap
    }

    /// Get the required votes (the `k`) to allow an upgrade for the k out of n policy.
    public fun required_votes(cap: &KofNUpgradeCap): u64 {
        cap.required_votes
    }

    /// Get the allowed voters (the `n` voters) for the k out of n policy.
    public fun voters(cap: &KofNUpgradeCap): &VecSet<address> {
        &cap.voters
    }

    /// Get the ID of the k out of n upgrade policy associated to the proposal.
    public fun proposal_for(proposal: &ProposedUpgrade): ID {
        proposal.cap
    }

    /// Get the upgrade proposer. 
    public fun proposer(proposal: &ProposedUpgrade): address {
        proposal.signer
    }

    /// Get the digest of the proposed upgrade.
    public fun digest(proposal: &ProposedUpgrade): &vector<u8> {
        &proposal.digest
    }

    /// Get the set of IDs of all the ballots issued for the given proposal.
    public fun allowed_voters(proposal: &ProposedUpgrade): &VecSet<ID> {
        &proposal.allowed_voters
    }

    /// Get the current accepted votes for the given proposal.
    public fun current_voters(proposal: &ProposedUpgrade): &VecSet<ID> {
        &proposal.current_voters
    }

    /// Get the original owner of the Ballot. That is the `address` that
    /// received the ballot initially.
    public fun owner(ballot: &Ballot): address {
        ballot.owner
    }

    /// Get the number of times the Ballot was transferred.
    public fun transfer_count(ballot: &Ballot): u64 {
        ballot.transfers_count
    }

    /// Get the digest of the proposed upgrade.
    public fun proposed_digest(ballot: &Ballot): &vector<u8> {
        &ballot.digest
    }

    /// Get the ID of the proposed upgrade.
    public fun proposed_upgrade(ballot: &Ballot): ID {
        ballot.proposed_upgrade
    }

    #[test_only]
    public fun change_ballot_digest(ballot: &mut Ballot, digest: vector<u8>) {
        ballot.digest = digest;
    }

    #[test_only]
    public fun change_ballot_proposed_upgrade(ballot: &mut Ballot, id: ID) {
        ballot.proposed_upgrade = id;
    }
}