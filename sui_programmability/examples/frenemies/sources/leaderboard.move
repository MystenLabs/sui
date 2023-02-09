// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module frenemies::leaderboard {
    use frenemies::assignment::{Self, Assignment};
    use frenemies::registry::Name;
    use std::vector;
    use sui::event;
    use sui::math;
    use sui::object::{Self, UID};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::table::{Self, Table};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::validator_set;
    use sui::validator;

    friend frenemies::frenemies;

    /// Singleton struct scoring staking information for recent epochs,
    /// start epoch, and a leaderborad with top scorers
    struct Leaderboard has key {
        id: UID,
        /// Top SCORE_MAX scores, sorted in ascending order
        top_scores: vector<Score>,
        /// Validator set sorted by stake in ascending order for each epoch.
        /// An entry in the this table at epoch N corresponds to the stakes at the end of epoch N (or beginning of epoch N + 1 if you prefer)
        /// This is (intentionally) ever-growing, but for a long-running competiton, pruning probably makes sense.
        prev_epoch_stakes: Table<u64, vector<Validator>>,
        /// Current epoch. Is never present in the keys of `prev_epoch_stakes` (since stakes for the end of this epoch are not yet known)
        epoch: u64,
        /// Epoch where the competition began--i.e., the lowest entry in the keys of `prev_epoch_stakes`
        /// Note: `prev_epoch_stakes` is empty until `start_epoch + 1`
        start_epoch: u64,
    }

    /// Running tally of a plaeyer's score
    struct Score has store, copy, drop {
        /// name of the player
        name: Name,
        /// the latest score
        score: u16,
        /// number of epochs the player has participated in--used as a tiebreaker for equal scores
        /// less participation = higher score (i.e., got there with fewer tries)
        participation: u16,
    }

    /// Information about a validator
    struct Validator has store, drop, copy {
        addr: address,
        stake: u64,
    }

    /// Event emitted each time a player's score is updated. Suitable for building an exhaustive leaderboard offchain
    struct NewScoreEvent has copy, drop {
        /// Name of the player
        name: Name,
        /// Score of the player
        score: Score,
        /// Epoch when this score was recorded. Note that a score from an assignment in epoch N might be reported in N + 1, N + 2, etc.
        epoch: u64
    }

    /// Number of scores kept in the leaderboard
    const LEADERBOARD_SIZE: u64 = 2_000;

    /// Number of points a player receives for successfully completing an epoch goal.
    /// Additional points are awarded according to the difficulty of the goal (i.e.,
    /// how many places the validator moved during the epoch)
    const GOAL_COMPLETION_POINTS: u16 = 10;

    fun init(ctx: &mut TxContext) {
        let epoch = tx_context::epoch(ctx);
        transfer::share_object(
            Leaderboard {
                id: object::new(ctx),
                top_scores: vector[],
                prev_epoch_stakes: table::new(ctx),
                epoch,
                start_epoch: epoch,
            }
        )
    }

    /// Sort active validators in `state` by stake and add to `leaderboard prev_epoch_stakes` so we can score
    /// incoming scorecards.
    /// Assumption: cur_epoch > self.start_epoch
    public(friend) fun update_epoch_info(self: &mut Leaderboard, state: &SuiSystemState, cur_epoch: u64) {
        let validators = vector[];
        let active_validators = validator_set::active_validators(sui_system::validators(state));
        let i = 0;
        let len = vector::length(active_validators);
        while (i < len) {
            let validator_data = vector::borrow(active_validators, i);
            validator_insertion_sort(
                &mut validators,
                Validator {
                    addr: validator::sui_address(validator_data),
                    stake: validator::total_stake(validator_data),
                }
            );
            i = i + 1
        };
        // stake info in `state` is the result of the *previous* epoch
        table::add(&mut self.prev_epoch_stakes, cur_epoch - 1, validators);
        self.epoch = cur_epoch
    }

    public(friend) fun update(
        self: &mut Leaderboard, name: Name, points: u16, participation: u16, cur_epoch: u64
    ) {
        let score = Score { name, score: points, participation };
        score_insertion_sort(&mut self.top_scores, score);
        // emit an event that can be used to construct a leaderboard in the frontend. we emit events for every score, not just the top ones
        event::emit(NewScoreEvent { name, score, epoch: cur_epoch })
    }

    /// Return the score associated with `assignment`
    /// Assumption: `self.previous_epoch_stakes[assignment.epoch]` is initialized
    public(friend) fun get_score(self: &Leaderboard, assignment: &Assignment): u16 {
        let epoch = assignment::epoch(assignment);
        if (!table::contains(&self.prev_epoch_stakes, epoch)) {
            // this should only happen if the "`update` called at least once/epoch" assumption described in `frenemies::update` is violated.
            // return a score of 0, since we have lost track of the staking information for assignment.epoch
            return 0
        };
        let validators = table::borrow(&self.prev_epoch_stakes, epoch);
        let validator = assignment::validator(assignment);
        let rank = find_validator(validators, validator);
        let outcome = assignment::get_outcome((rank as u64), vector::length(validators));
        if (assignment::goal(assignment) == outcome) {
            let difficulty = if (epoch == self.start_epoch || !table::contains(&self.prev_epoch_stakes, epoch - 1)) {
                // no previous rank--define difficulty as 0. this should only happen in two cases:
                // 1. epoch - 1 = start_epoch
                // 2. the "`update` called at least once/epoch" assumption described in `frenemies::update` is violated
                0
            } else {
                // difficulty is defined as the change in the player's rank during the epoch
                // that means the player gets more points for moving the validator a lot (e.g., worst-to-first)
                // however, this also has some quirks; e.g., if your goal is FRIEND and your validator is already
                // ranked first, you get max points by moving your validator to the bottom of the FRIEND cutoff
                // player gets base score for completing the goal + bonus points for difficulty
                let prev_rank = find_validator(table::borrow(&self.prev_epoch_stakes, epoch - 1), validator);
                (math::diff((rank as u64), (prev_rank as u64)) as u16)
            };
            GOAL_COMPLETION_POINTS + difficulty
        } else {
            // didn't meet the goal, no points
            0
        }
    }

     /// insert `validator` into `v`, maintaining the invariant that `v` is in descending order by stake
    fun validator_insertion_sort(v: &mut vector<Validator>, validator: Validator) {
        let stake = validator.stake;
        let i = 0;
        let len = vector::length(v);
        while (i < len) {
            if (stake > vector::borrow(v, i).stake) {
                vector::insert(v, validator, i);
                return
            };
            i = i + 1
        };
        vector::push_back(v, validator)
    }

    /// write `s` into `v`, maintaining the invariant that `v` is in descending order by (score, participation)
    // preconditions:
    // - v has at most one entry such that v[_].name == new_score.name. we enforce this below.
    // - if there is such an entry, new_score.score > v[_].score. this is because we only call `score_insertion_sort` when the player's score increases
    fun score_insertion_sort(v: &mut vector<Score>, new_score: Score) {
        let len = vector::length(v);
        let at_capacity = len == LEADERBOARD_SIZE || len > LEADERBOARD_SIZE;
        let new_high_score = len == 0 || gt(&new_score, vector::borrow(v, len - 1));
        if (!new_high_score) {
            if (!at_capacity) {
                vector::push_back(v, new_score)
            };
            return
        };
        // we know `new_score` is greater than an existing score. walk backward to find its place
        let i = len;
        while (i > 0) {
            i = i - 1;
            let s = vector::borrow_mut(v, i);
             // ensure that `v` has at most one entry per player
            if (s.name == new_score.name) {
                vector::remove(v, i);
            } else if (gt(s, &new_score)) {
                vector::insert(v, new_score, i + 1);
                if (vector::length(v) > LEADERBOARD_SIZE) {
                    // pop off a low score to make room
                    vector::pop_back(v);
                };
                return
            };
        };
        // new top score!
        vector::insert(v, new_score, 0);
        vector::pop_back(v)
    }

    fun gt(s1: &Score, s2: &Score): bool {
        s1.score > s2.score || (s1.score == s2.score && s1.participation < s2.participation)
    }

    /// Return the index of the validator with address `addr` in `v`
    fun find_validator(v: &vector<Validator>, addr: address): u8 {
        let i = 0;
        let len = vector::length(v);
        while (i < len) {
            let e = vector::borrow(v, i);
            if (e.addr == addr) {
                return (i as u8)
            };
            i = i + 1
        };
        // unreachable with static validator set
        abort(0)
    }

    /// Return live stake info based on pending delegations and withdrawals
    /// Intended for client usage, helpful if you want to see how your assigned
    /// validator is doing in the middle of an epoch
    /// Note: assumes static validator set
    public fun next_epoch_stakes(state: &SuiSystemState): vector<Validator> {
        let validators = validator_set::active_validators(sui_system::validators(state));
        let next_epoch_stakes = vector[];
        let i = 0;
        let num_validators = vector::length(validators);
        while (i < num_validators) {
            let validator = vector::borrow(validators, i);
            let addr = validator::sui_address(validator);
            let stake = validator::total_stake(validator) + validator::pending_stake_amount(validator) - validator::pending_withdraw(validator);
            validator_insertion_sort(&mut next_epoch_stakes, Validator { addr, stake });
            i = i + 1
        };
        next_epoch_stakes
    }

    /// Return the evalidator stake rankings
    public fun last_epoch_stakes(self: &Leaderboard): &vector<Validator> {
        table::borrow(&self.prev_epoch_stakes, self.epoch - 1)
    }

    public fun start_epoch(self: &Leaderboard): u64 {
        self.start_epoch
    }

    public fun epoch(self: &Leaderboard): u64 {
        self.epoch
    }

    public fun top_scores(self: &Leaderboard): &vector<Score> {
        &self.top_scores
    }

    public fun prev_epoch_stakes(self: &Leaderboard): &Table<u64, vector<Validator>> {
        &self.prev_epoch_stakes
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx)
    }

    #[test_only]
    public fun score_for_testing(name: std::string::String, score: u16, participation: u16): Score {
        Score { name: frenemies::registry::name_for_testing(name), score, participation }
    }

    #[test]
    fun test_score_sort() {
        use frenemies::registry;
        use std::string;

        let score1 = Score { name: registry::name_for_testing(string::utf8(b"A")), score: 20, participation: 1 };
        let score2 = Score { name: registry::name_for_testing(string::utf8(b"B")), score: 10, participation: 1 };
        let score3 = Score { name: registry::name_for_testing(string::utf8(b"C")), score: 10, participation: 2 };

        assert!(gt(&score1, &score2), 0);
        assert!(gt(&score2, &score3), 0);
        assert!(gt(&score1, &score3), 0);

        let scores = vector[score2, score3, score1];
        let i = 0;
        let sorted = vector[];
        let len = vector::length(&scores);
        while (i < len) {
            let score = vector::pop_back(&mut scores);
            score_insertion_sort(&mut sorted, score);
            i = i + 1
        };
        assert!(sorted == vector[score1, score2, score3], 0);
        let new_top_score = Score { name: registry::name_for_testing(string::utf8(b"B")), score: 25, participation: 2 };
        // insert new score with same id
        score_insertion_sort(&mut sorted, new_top_score);
        assert!(sorted == vector[new_top_score, score1, score3], 0);

        // insert new bottom score with same id
        let new_bottom_score = Score { name: registry::name_for_testing(string::utf8(b"C")), score: 15, participation: 3 };
        // insert new score with same id
        score_insertion_sort(&mut sorted, new_bottom_score);
        assert!(sorted == vector[new_top_score, score1, new_bottom_score], 0);

        // insert new middle score with same id
        let new_middle_score = Score { name: registry::name_for_testing(string::utf8(b"A")), score: 21, participation: 2 };
        // insert new score with same id
        score_insertion_sort(&mut sorted, new_middle_score);
        assert!(sorted == vector[new_top_score, new_middle_score, new_bottom_score], 0);
    }

    #[test]
    fun test_validator_sort() {
        let v1 = Validator { addr: @0x1, stake: 100 };
        let v2 = Validator { addr: @0x2, stake: 90 };
        let v3 = Validator { addr: @0x3, stake: 75 };
        let v4 = Validator { addr: @0x4, stake: 20 };

        let validators = vector[v2, v1, v4, v3];
        let sorted = vector[];
        let len = vector::length(&validators);
        let i = 0;
        while (i < len) {
            let validator = vector::pop_back(&mut validators);
            validator_insertion_sort(&mut sorted, validator);
            i = i + 1;
        };
        assert!(sorted == vector[v1, v2, v3, v4], 0)
    }
}
