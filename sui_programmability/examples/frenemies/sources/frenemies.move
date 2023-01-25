// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module frenemies::frenemies {
    use frenemies::assignment::{Self, Assignment};
    use frenemies::registry::{Self, Name, Registry};
    use frenemies::leaderboard::{Self, Leaderboard};
    use std::string::String;
    use sui::event;
    use sui::object::{Self, ID, UID};
    use sui::sui_system::SuiSystemState;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Non-transferrable scorecard
    struct Scorecard has key {
        id: UID,
        /// Name of the player, enforced to be globally unique via the Registry
        name: Name,
        /// Current assignment
        assignment: Assignment,
        /// Accumulated score across epochs
        score: u16,
        /// Number of epochs for which the player received a score (even if the score was 0)
        participation: u16,
        /// Latest epoch for which an assignment has been recorded, but a score has not yet been assigned
        epoch: u64
    }

    /// Emitted each time a user updates their scorecard
    struct ScorecardUpdateEvent has copy, drop {
        /// ID of the player's `Scorecard`
        scorecard: ID,
        /// player's assignment for the epoch
        assignment: Assignment,
        /// player's total score after scoring `assignment`
        total_score: u16,
        /// score for the epoch. 0 if the player was not successful
        epoch_score: u16,
    }

    /// Attempting to call `update` with an epoch N assignment during epoch N
    const EScoreNotYetAvailable: u64 = 0;

    /// Register the transaction sender for the frenemies game by sending them a 0 scorecard with
    /// an initial assignment
    public entry fun register(name: String, registry: &mut Registry, state: &SuiSystemState, ctx: &mut TxContext) {
        let sender = tx_context::sender(ctx);
        transfer::transfer(
             Scorecard {
                id: object::new(ctx),
                name: registry::register(registry, name, ctx),
                assignment: assignment::get(state, ctx),
                score: 0,
                participation: 0,
                epoch: tx_context::epoch(ctx),
            },
            sender
        )
    }

    /// Update `scorecard` with the results from the last epoch, update `leaderboard` if this is a high score, update
    /// `scorecard` with a new assignment for
    /// Note: if scorecard_epoch is 7 and it is now epoch 10, the player will get a score for 7, but not for 8 or 9.
    /// the player never got assignments for those epochs, so they could not have playing and cannot score.
    /// get new assignment for the current epoch
    /// Note: relies on someone calling this at least once/epoch to preserve
    /// the record of validator scores
    public entry fun update(
        scorecard: &mut Scorecard, state: &SuiSystemState, leaderboard: &mut Leaderboard, ctx: &TxContext
    ) {
        let cur_epoch = tx_context::epoch(ctx);
        // Can only get a score for an assignment during epoch N during epoch N + 1
        assert!(cur_epoch > assignment::epoch(&scorecard.assignment), EScoreNotYetAvailable);
        if (cur_epoch > leaderboard::epoch(leaderboard)) {
            // if first call in new epoch, update leaderboard with stake rankings for the previous epoch.
            // Note: there's a pragmatically safe, but theoretically unsafe assumption here that
            // `cur_epoch >= leaderboard.epoch + 1` because at least one player will update their scorecard
            // each epoch. if this assumption is violated via 0 calls to `update` during epoch `N`, players
            // will not get credit for scores during epoch `N - 1`.
            leaderboard::update_epoch_info(leaderboard, state, cur_epoch)
        };
        // update scorecard, get new assignment for the current epoch
        scorecard.participation = scorecard.participation + 1;
        scorecard.epoch = cur_epoch + 1;
        let epoch_score = leaderboard::get_score(leaderboard, &scorecard.assignment);
        if (epoch_score != 0) {
            let new_score = scorecard.score + epoch_score;
            scorecard.score = new_score;
            leaderboard::update(leaderboard, scorecard.name, new_score, scorecard.participation, cur_epoch);
            event::emit(ScorecardUpdateEvent { scorecard: object::id(scorecard), assignment: scorecard.assignment, epoch_score, total_score: new_score })
        } else {
            event::emit(ScorecardUpdateEvent { scorecard: object::id(scorecard), assignment: scorecard.assignment, epoch_score: 0, total_score: scorecard.score })
        };
        // TODO: move into update_scorecard by making get_assignment work off of `state`
        scorecard.assignment = assignment::get(state, ctx);
    }

    /// Return the name associated with this scorecard
    public fun name(self: &Scorecard): &Name {
        &self.name
    }

    /// Return the total score for `scorecard`
    public fun score(self: &Scorecard): u16 {
        self.score
    }

     /// Return the total score for `scorecard`
    public fun participation(self: &Scorecard): u16 {
        self.participation
    }

    /// Return the last scored epoch for `scorecard`
    public fun epoch(self: &Scorecard): u64 {
        self.epoch
    }

    /// Return the current assignment for `scorecard`
    public fun assignment(self: &Scorecard): &Assignment {
        &self.assignment
    }

    /// Return the current validator for `scorecard`
    public fun validator(self: &Scorecard): address {
        assignment::validator(&self.assignment)
    }

    /// Return the current goal for `scorecard`
    public fun goal(self: &Scorecard): u8 {
        assignment::goal(&self.assignment)
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        leaderboard::init_for_testing(ctx);
        registry::init_for_testing(ctx)
    }

    #[test_only]
    public fun set_assignment_for_testing(scorecard: &mut Scorecard, validator: address, assignment: u8, epoch: u64) {
        scorecard.assignment = assignment::new_for_testing(validator, assignment, epoch)
    }
}
