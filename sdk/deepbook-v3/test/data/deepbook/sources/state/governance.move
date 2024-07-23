// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Governance module handles the governance of the `Pool` that it's attached to.
/// Users with non zero stake can create proposals and vote on them. Winning
/// proposals are used to set the trade parameters for the next epoch.
module deepbook::governance {
    // === Imports ===
    use sui::vec_map::{Self, VecMap};
    use deepbook::{trade_params::{Self, TradeParams}, constants, math};

    // === Errors ===
    const EInvalidMakerFee: u64 = 1;
    const EInvalidTakerFee: u64 = 2;
    const EProposalDoesNotExist: u64 = 3;
    const EMaxProposalsReachedNotEnoughVotes: u64 = 4;
    const EAlreadyProposed: u64 = 5;
    const EWhitelistedPoolCannotChange: u64 = 6;

    // === Constants ===
    const MIN_TAKER_STABLE: u64 = 50000; // 0.5 basis points
    const MAX_TAKER_STABLE: u64 = 100000;
    const MIN_MAKER_STABLE: u64 = 20000;
    const MAX_MAKER_STABLE: u64 = 50000;
    const MIN_TAKER_VOLATILE: u64 = 500000;
    const MAX_TAKER_VOLATILE: u64 = 1000000;
    const MIN_MAKER_VOLATILE: u64 = 200000;
    const MAX_MAKER_VOLATILE: u64 = 500000;
    const MAX_PROPOSALS: u64 = 100;
    const VOTING_POWER_THRESHOLD: u64 = 100_000_000_000; // 100k deep

    // === Structs ===
    /// `Proposal` struct that holds the parameters of a proposal and its current total votes.
    public struct Proposal has store, drop, copy {
        taker_fee: u64,
        maker_fee: u64,
        stake_required: u64,
        votes: u64,
    }

    /// Details of a pool. This is refreshed every epoch by the first
    /// `State` action against this pool.
    public struct Governance has store {
        /// Tracks refreshes.
        epoch: u64,
        /// If Pool is whitelisted.
        whitelisted: bool,
        /// If Pool is stable or volatile.
        stable: bool,
        /// List of proposals for the current epoch.
        proposals: VecMap<ID, Proposal>,
        /// Trade parameters for the current epoch.
        trade_params: TradeParams,
        /// Trade parameters for the next epoch.
        next_trade_params: TradeParams,
        /// All voting power from the current stakes.
        voting_power: u64,
        /// Quorum for the current epoch.
        quorum: u64,
    }

    // === Public-Package Functions ===
    public(package) fun empty(stable_pool: bool, ctx: &TxContext): Governance {
        let default_taker = if (stable_pool) { MAX_TAKER_STABLE } else { MAX_TAKER_VOLATILE };
        let default_maker = if (stable_pool) { MAX_MAKER_STABLE } else { MAX_MAKER_VOLATILE };
        Governance {
            epoch: ctx.epoch(),
            whitelisted: false,
            stable: stable_pool,
            proposals: vec_map::empty(),
            trade_params: trade_params::new(
                default_taker,
                default_maker,
                constants::default_stake_required(),
            ),
            next_trade_params: trade_params::new(
                default_taker,
                default_maker,
                constants::default_stake_required(),
            ),
            voting_power: 0,
            quorum: 0,
        }
    }

    /// Whitelist a pool. This pool can be used as a DEEP reference price for
    /// other pools. This pool will have zero fees.
    public(package) fun set_whitelist(self: &mut Governance, whitelisted: bool) {
        self.whitelisted = whitelisted;
        self.stable = false;
        self.reset_trade_params();
    }

    public(package) fun whitelisted(self: &Governance): bool {
        self.whitelisted
    }

    /// Set the pool to stable or volatile. If stable, the fees are set to
    /// stable fees. If volatile, the fees are set to volatile fees.
    /// This resets governance. A whitelisted pool cannot be set to stable.
    public(package) fun set_stable(self: &mut Governance, stable: bool) {
        assert!(!self.whitelisted, EWhitelistedPoolCannotChange);

        self.stable = stable;
        self.reset_trade_params();
    }

    /// Update the governance state. This is called at the start of every epoch.
    public(package) fun update(self: &mut Governance, ctx: &TxContext) {
        let epoch = ctx.epoch();
        if (self.epoch == epoch) return;

        self.epoch = epoch;
        self.quorum = math::mul(self.voting_power, constants::half());
        self.proposals = vec_map::empty();
        self.trade_params = self.next_trade_params;
    }

    /// Add a new proposal to governance.
    /// Check if proposer already voted, if so will give error.
    /// If proposer has not voted, and there are already MAX_PROPOSALS proposals,
    /// remove the proposal with the lowest votes if it has less votes than the voting power.
    /// Validation of the account adding is done in `State`.
    public(package) fun add_proposal(
        self: &mut Governance,
        taker_fee: u64,
        maker_fee: u64,
        stake_required: u64,
        stake_amount: u64,
        account_id: ID,
    ) {
        assert!(!self.proposals.contains(&account_id), EAlreadyProposed);
        assert!(!self.whitelisted, EWhitelistedPoolCannotChange);

        if (self.stable) {
            assert!(
                taker_fee >= MIN_TAKER_STABLE && taker_fee <= MAX_TAKER_STABLE,
                EInvalidTakerFee,
            );
            assert!(
                maker_fee >= MIN_MAKER_STABLE && maker_fee <= MAX_MAKER_STABLE,
                EInvalidMakerFee,
            );
        } else {
            assert!(
                taker_fee >= MIN_TAKER_VOLATILE && taker_fee <= MAX_TAKER_VOLATILE,
                EInvalidTakerFee,
            );
            assert!(
                maker_fee >= MIN_MAKER_VOLATILE && maker_fee <= MAX_MAKER_VOLATILE,
                EInvalidMakerFee,
            );
        };

        let voting_power = stake_to_voting_power(stake_amount);
        if (self.proposals.size() == MAX_PROPOSALS) {
            self.remove_lowest_proposal(voting_power);
        };

        let new_proposal = new_proposal(taker_fee, maker_fee, stake_required);
        self.proposals.insert(account_id, new_proposal);
    }

    /// Vote on a proposal. Validation of the account and stake is done in `State`.
    /// If `from_proposal_id` is some, the account is removing their vote from that proposal.
    /// If `to_proposal_id` is some, the account is voting for that proposal.
    public(package) fun adjust_vote(
        self: &mut Governance,
        from_proposal_id: Option<ID>,
        to_proposal_id: Option<ID>,
        stake_amount: u64,
    ) {
        let votes = stake_to_voting_power(stake_amount);

        if (from_proposal_id.is_some() && self.proposals.contains(from_proposal_id.borrow())) {
            let proposal = &mut self.proposals[from_proposal_id.borrow()];
            proposal.votes = proposal.votes - votes;
            if (proposal.votes + votes > self.quorum && proposal.votes < self.quorum) {
                self.next_trade_params = self.trade_params;
            };
        };

        if (to_proposal_id.is_some()) {
            assert!(self.proposals.contains(to_proposal_id.borrow()), EProposalDoesNotExist);

            let proposal = &mut self.proposals[to_proposal_id.borrow()];
            proposal.votes = proposal.votes + votes;
            if (proposal.votes > self.quorum) {
                self.next_trade_params = proposal.to_trade_params();
            };
        };
    }

    /// Adjust the total voting power by adding and removing stake. For example, if an account's
    /// stake goes from 2000 to 3000, then `stake_before` is 2000 and `stake_after` is 3000.
    /// Validation of inputs done in `State`.
    public(package) fun adjust_voting_power(
        self: &mut Governance,
        stake_before: u64,
        stake_after: u64,
    ) {
        self.voting_power = self.voting_power +
        stake_to_voting_power(stake_after) -
        stake_to_voting_power(stake_before);
    }

    public(package) fun trade_params(self: &Governance): TradeParams {
        self.trade_params
    }

    // === Private Functions ===
    /// Convert stake to voting power.
    fun stake_to_voting_power(stake: u64): u64 {
        let mut voting_power = math::min(stake, VOTING_POWER_THRESHOLD);
        if (stake > VOTING_POWER_THRESHOLD) {
            voting_power = voting_power + math::sqrt(stake, constants::deep_unit()) -
            math::sqrt(VOTING_POWER_THRESHOLD, constants::deep_unit());
        };

        voting_power
    }

    fun new_proposal(taker_fee: u64, maker_fee: u64, stake_required: u64): Proposal {
        Proposal { taker_fee, maker_fee, stake_required, votes: 0 }
    }

    /// Remove the proposal with the lowest votes if it has less votes than the voting power.
    /// If there are multiple proposals with the same lowest votes, the latest one is removed.
    fun remove_lowest_proposal(self: &mut Governance, voting_power: u64) {
        let mut removal_id = option::none<ID>();
        let mut cur_lowest_votes = constants::max_u64();
        let (keys, values) = self.proposals.into_keys_values();
        let mut i = 0;

        while (i < self.proposals.size()) {
            let proposal_votes = values[i].votes;
            if (proposal_votes < voting_power && proposal_votes <= cur_lowest_votes) {
                removal_id = option::some(keys[i]);
                cur_lowest_votes = proposal_votes;
            };
            i = i + 1;
        };

        assert!(removal_id.is_some(), EMaxProposalsReachedNotEnoughVotes);
        self.proposals.remove(removal_id.borrow());
    }

    fun reset_trade_params(self: &mut Governance) {
        self.proposals = vec_map::empty();
        let stake = self.trade_params.stake_required();
        if (self.whitelisted) {
            self.trade_params = trade_params::new(0, 0, 0);
        } else if (self.stable) {
            self.trade_params = trade_params::new(MAX_TAKER_STABLE, MAX_MAKER_STABLE, stake);
        } else {
            self.trade_params = trade_params::new(MAX_TAKER_VOLATILE, MAX_MAKER_VOLATILE, stake);
        };
        self.next_trade_params = self.trade_params;
    }

    fun to_trade_params(proposal: &Proposal): TradeParams {
        trade_params::new(proposal.taker_fee, proposal.maker_fee, proposal.stake_required)
    }

    // === Test Functions ===
    #[test_only]
    public fun voting_power(self: &Governance): u64 {
        self.voting_power
    }

    #[test_only]
    public fun stable(self: &Governance): bool {
        self.stable
    }

    #[test_only]
    public fun quorum(self: &Governance): u64 {
        self.quorum
    }

    #[test_only]
    public fun proposals(self: &Governance): VecMap<ID, Proposal> {
        self.proposals
    }

    #[test_only]
    public fun next_trade_params(self: &Governance): TradeParams {
        self.next_trade_params
    }

    #[test_only]
    public fun votes(proposal: &Proposal): u64 {
        proposal.votes
    }

    #[test_only]
    public fun params(proposal: &Proposal): (u64, u64, u64) {
        (proposal.taker_fee, proposal.maker_fee, proposal.stake_required)
    }
}
