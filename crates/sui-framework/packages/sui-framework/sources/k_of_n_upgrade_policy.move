// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Simple upgrade policy that requires a `k` out of `n` quorum in order to perform
/// a proposed upgrade.
/// 
/// This policy is initiated with a call to `k_of_n_upgrade_policy::new` providing 
/// the `UpgradeCap` of the package to be controlled by the policy, the `k` value 
/// (number of votes to be received for the upgrade to be allowed) and the list of
/// `address`es allowed to vote. The `address`es provided will receive
/// a `VotingCap` that allows them to vote. 
/// This policy can be created at any point during the lifetime of the package upgrade 
/// cap.
/// 
/// An upgrade is proposed via `k_of_n_upgrade_policy::propose_upgrade` and saved as 
/// a shared object.
/// Once the number of votes is reached the proposer of the upgrade can perform
/// the upgrade.
/// 
/// Events are emitted to track the main operations on the proposal.
/// A proposed upgrade lifetime is tracked via the 4 events:
/// `UpgradeProposed`, `UpgradeVoted` and `UpgradePerformed` or `UpgradeDiscarded`.
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
        upgrade_cap: UpgradeCap,
        /// Number of votes required for the upgrade to be allowed.
        required_votes: u64,
        /// Allowed voters.
        voters: VecSet<address>,
        /// Voting caps issued.
        voter_caps: VecSet<ID>,
    }

    /// A capability to vote an upgrade.
    /// Sent to each registered address when a new upgrade is created.
    /// Receiving parties will use the capability to vote for the upgrade. 
    struct VotingCap has key {
        id: UID,
        /// The original address the capability was sent to.
        owner: address,
        /// The ID of the `KofNUpgradeCap` this capability refers to.
        upgrade_cap: ID,
        /// The count of transfers this capability went through. 
        /// It is informational only and can be used to track transfers of
        /// voter capability instances.
        transfers_count: u64,
        /// The number of votes issued by this voter.
        votes_issued: u64,
    }

    /// A proposed upgrade that is going through voting.
    /// `ProposedUpgrade` instances are shared objects that will be passed as 
    /// an argument, together with a `VotingCap`, when voting.
    /// It's possible to have multiple proposed upgrades at the same time and 
    /// the first successful update will obsolete all the others, given
    /// an attempt to upgrade with a "concurrent" one will fail because of
    /// versioning.
    struct ProposedUpgrade has key {
        id: UID,
        /// The ID of the `KofNUpgradeCap` that this vote was initiated from.
        upgrade_cap: ID,
        /// The address requesting permission to perform the upgrade.
        /// This is the sender of the transaction that proposes and 
        /// performs the upgrade.
        proposer: address,
        /// The digest of the bytecode that the package will be upgraded to.
        digest: vector<u8>,
        /// The current voters that have accepted the upgrade.
        current_voters: VecSet<ID>,
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
    }

    struct UpgradeVoted has copy, drop {
        /// the ID of the proposal (`ProposedUpgrade` instance)
        proposal: ID,
        /// digest of the proposal
        digest: vector<u8>,
        /// the ID of the voter (VotingCap instance)
        voter: ID,
        /// The signer of the transaction that voted.
        signer: address,
    }

    /// A succesful upgrade.
    struct UpgradePerformed has copy, drop {
        /// the instance of the k out of n policy
        upgrade_cap: ID,
        /// the ID of the proposal (`ProposedUpgrade` instance)
        proposal: ID,
        /// digest of the proposal
        digest: vector<u8>,
        /// proposer of the upgrade
        proposer: address,
    }

    /// A discarded upgrade.
    struct UpgradeDiscarded has copy, drop {
        /// the instance of the k out of n policy
        upgrade_cap: ID,
        /// the ID of the proposal (`ProposedUpgrade` instance)
        proposal: ID,
        /// digest of the proposal
        digest: vector<u8>,
        /// proposer of the upgrade
        proposer: address,
    }

    /// Allowed voters must in the [2, 100] range.
    const EAllowedVotersError: u64 = 0;
    /// Required votes must be less than allowed voters.
    const ERequiredVotesError: u64 = 1;
    /// An upgrade was issued already, and the operation requested failed.
    const EAlreadyIssued: u64 = 2;
    /// The given `VotingCap` is not for the given `ProposedUpgrade`
    const EInvalidVoterForUpgrade: u64 = 3;
    /// The given capability owner already voted.
    const EAlreadyVoted: u64 = 4;
    /// Not enough votes to perform the upgrade.
    const ENotEnoughVotes: u64 = 5;
    /// The operation required the signer to be the same as the upgrade proposer.
    const ESignerMismatch: u64 = 6;
    /// Proposal (`KofNUpgradeCap`) and upgrade (`ProposedUpgrade`) do not match.
    const EInvalidProposalForUpgrade: u64 = 7;

    /// Create a `KofNUpgradeCap` given an `UpgradeCap`.
    /// The returned instance is the only and exclusive controller of upgrades. 
    /// The `k` (`required_votes`) out of `n` (length of `voters`) is set up
    /// at construction time and it is immutable.
    public fun new(
        upgrade_cap: UpgradeCap,
        required_votes: u64,
        voters: VecSet<address>,
        ctx: &mut TxContext,
    ): KofNUpgradeCap {
        // currently the allowed voters is limited to 100 and the number of
        // required votes must be at least 2 and less or equal than the number of voters
        assert!(vec_set::size(&voters) > 1, EAllowedVotersError);
        assert!(vec_set::size(&voters) <= 100, EAllowedVotersError);
        assert!(required_votes > 0, ERequiredVotesError);
        assert!(required_votes <= vec_set::size(&voters), ERequiredVotesError);

        // upgrade cap id
        let cap_uid = object::new(ctx);
        let cap_id = object::uid_to_inner(&cap_uid);

        let voter_caps: VecSet<ID> = vec_set::empty();
        let voter_addresses = vec_set::keys(&voters);
        let voter_idx = vector::length(voter_addresses);
        while (voter_idx > 0) {
            voter_idx = voter_idx - 1;
            let address = *vector::borrow(voter_addresses, voter_idx);
            let voter_uid = object::new(ctx);
            let voter_id = object::uid_to_inner(&voter_uid);
            transfer::transfer(
                VotingCap {
                    id: voter_uid,
                    owner: address,
                    upgrade_cap: cap_id,
                    transfers_count: 0,
                    votes_issued: 0,
                },
                address,
            );
            vec_set::insert(&mut voter_caps, voter_id);
        };

        KofNUpgradeCap {
            id: cap_uid,
            upgrade_cap,
            required_votes,
            voters,
            voter_caps,
        }
    }

    /// Make the package immutable by destroying the k of n upgrade cap and the
    /// underlying upgrade cap.
    public fun make_immutable(cap: KofNUpgradeCap) {
        let KofNUpgradeCap {
            id,
            upgrade_cap,
            required_votes: _,
            voters: _,
            voter_caps: _,
        } = cap;
        object::delete(id);
        package::make_immutable(upgrade_cap);
    }

    /// Restrict upgrades to "add code only", or "change dependencies".
    public fun only_additive_upgrades(cap: &mut KofNUpgradeCap) {
        package::only_additive_upgrades(&mut cap.upgrade_cap)
    }

    /// Restrict upgrades to "change dependencies only".
    public fun only_dep_upgrades(cap: &mut KofNUpgradeCap) {
        package::only_dep_upgrades(&mut cap.upgrade_cap)
    }

    /// Propose an upgrade. 
    /// The `digest` of the proposed upgrade is provided to identify the upgrade.
    /// The proposer is the sender of the transaction and must be the signer
    /// of the commit transaction as well.
    public fun propose_upgrade(
        cap: &KofNUpgradeCap,
        digest: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let cap_id = object::id(cap);
        let proposal_uid = object::new(ctx);
        let proposal_id = object::uid_to_inner(&proposal_uid);
        
        let proposer = tx_context::sender(ctx);
        
        event::emit(UpgradeProposed {
            upgrade_cap: cap_id,
            proposal: proposal_id,
            digest,
            proposer,
            voters: cap.voters,
        });

        transfer::share_object(ProposedUpgrade {
            id: proposal_uid,
            upgrade_cap: cap_id,
            proposer,
            digest,
            current_voters: vec_set::empty(),
        })
    }

    /// Vote in favor of an upgrade, aborts if the voter is not for the proposed
    /// upgrade or if they voted already, or if the upgrade was already performed.
    public fun vote(
        proposal: &mut ProposedUpgrade, 
        voter: &mut VotingCap,
        ctx: &TxContext,
    ) {
        assert!(proposal.proposer != @0x0, EAlreadyIssued);
        assert!(proposal.upgrade_cap == voter.upgrade_cap, EInvalidVoterForUpgrade);
        let voter_id = object::id(voter);
        assert!(
            !vec_set::contains(&proposal.current_voters, &voter_id), 
            EAlreadyVoted,
        );
        vec_set::insert(&mut proposal.current_voters, voter_id);
        voter.votes_issued = voter.votes_issued + 1;

        event::emit(UpgradeVoted {
            proposal: object::id(proposal),
            digest: proposal.digest,
            voter: voter_id,
            signer: tx_context::sender(ctx),
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
        assert!(proposal.upgrade_cap == object::id(cap), EInvalidProposalForUpgrade);
        assert!(
            vec_set::size(&proposal.current_voters) >= cap.required_votes, 
            ENotEnoughVotes,
        );
        assert!(proposal.proposer != @0x0, EAlreadyIssued);

        // assert the signer is the proposer and the upgrade has not happened yet
        let signer = tx_context::sender(ctx);
        assert!(proposal.proposer == signer, ESignerMismatch);
        proposal.proposer = @0x0;

        event::emit(UpgradePerformed {
            upgrade_cap: proposal.upgrade_cap,
            proposal: object::id(proposal),
            digest: proposal.digest,
            proposer: signer,
        });

        let policy = package::upgrade_policy(&cap.upgrade_cap);
        package::authorize_upgrade(
            &mut cap.upgrade_cap,
            policy,
            proposal.digest,
        )
    }

    /// Finalize the upgrade to produce the given receipt.
    public fun commit_upgrade(
        cap: &mut KofNUpgradeCap, 
        receipt: UpgradeReceipt,
    ) {
        package::commit_upgrade(&mut cap.upgrade_cap, receipt)
    }

    /// Discard an existing proposed upgrade.
    /// The signer of the transaction must be the same address that proposed the
    /// upgrade.
    public fun discard_proposed_upgrade(proposed_upgrade: ProposedUpgrade, ctx: &TxContext) {
        let proposal = object::id(&proposed_upgrade);
        let ProposedUpgrade {
            id,
            upgrade_cap,
            proposer,
            digest,
            current_voters: _,
        } = proposed_upgrade;
        assert!(proposer == tx_context::sender(ctx), ESignerMismatch);
        event::emit(UpgradeDiscarded {
            upgrade_cap,
            proposal,
            digest,
            proposer,
        });
        object::delete(id);
    }

    //
    // Accessors
    //

    /// Get the `UpgradeCap` of the package protected by the policy.
    public fun upgrade_cap(cap: &KofNUpgradeCap): &UpgradeCap {
        &cap.upgrade_cap
    }

    /// Get the number of required votes for an upgrade to be valid.
    public fun required_votes(cap: &KofNUpgradeCap): u64 {
        cap.required_votes
    }

    /// Get the allowed voters for the policy.
    public fun voters(cap: &KofNUpgradeCap): &VecSet<address> {
        &cap.voters
    }

    /// Get the ID of the policy associated to the proposal.
    public fun proposal_for(proposal: &ProposedUpgrade): ID {
        proposal.upgrade_cap
    }

    /// Get the upgrade proposer. 
    public fun proposer(proposal: &ProposedUpgrade): address {
        proposal.proposer
    }

    /// Get the digest of the proposed upgrade.
    public fun digest(proposal: &ProposedUpgrade): &vector<u8> {
        &proposal.digest
    }

    /// Get the current accepted votes for the given proposal.
    public fun current_voters(proposal: &ProposedUpgrade): &VecSet<ID> {
        &proposal.current_voters
    }
}