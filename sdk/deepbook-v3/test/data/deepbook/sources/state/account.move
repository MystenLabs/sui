// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Account module manages the account data for each user.
module deepbook::account {
    // === Imports ===
    use sui::vec_set::{Self, VecSet};
    use deepbook::{fill::Fill, balances::{Self, Balances}};

    // === Structs ===
    /// Account data that is updated every epoch.
    /// One Account struct per BalanceManager object.
    public struct Account has store, copy, drop {
        epoch: u64,
        open_orders: VecSet<u128>,
        taker_volume: u64,
        maker_volume: u64,
        active_stake: u64,
        inactive_stake: u64,
        voted_proposal: Option<ID>,
        unclaimed_rebates: Balances,
        settled_balances: Balances,
        owed_balances: Balances,
    }

    // === Public-Package Functions ===
    public(package) fun empty(ctx: &TxContext): Account {
        Account {
            epoch: ctx.epoch(),
            open_orders: vec_set::empty(),
            taker_volume: 0,
            maker_volume: 0,
            active_stake: 0,
            inactive_stake: 0,
            voted_proposal: option::none(),
            unclaimed_rebates: balances::empty(),
            settled_balances: balances::empty(),
            owed_balances: balances::empty(),
        }
    }

    public(package) fun active_stake(self: &Account): u64 {
        self.active_stake
    }

    public(package) fun inactive_stake(self: &Account): u64 {
        self.inactive_stake
    }

    public(package) fun total_volume(self: &Account): u64 {
        self.taker_volume + self.maker_volume
    }

    public(package) fun voted_proposal(self: &Account): Option<ID> {
        self.voted_proposal
    }

    public(package) fun open_orders(self: &Account): VecSet<u128> {
        self.open_orders
    }

    /// Update the account data for the new epoch.
    /// Returns the previous epoch, maker volume, and active stake.
    public(package) fun update(self: &mut Account, ctx: &TxContext): (u64, u64, u64) {
        if (self.epoch == ctx.epoch()) return (0, 0, 0);

        let prev_epoch = self.epoch;
        let prev_maker_volume = self.maker_volume;
        let prev_active_stake = self.active_stake;

        self.epoch = ctx.epoch();
        self.maker_volume = 0;
        self.taker_volume = 0;
        self.active_stake = self.active_stake + self.inactive_stake;
        self.inactive_stake = 0;
        self.voted_proposal = option::none();

        (prev_epoch, prev_maker_volume, prev_active_stake)
    }

    /// Given a fill, update the account balances and volumes as the maker.
    public(package) fun process_maker_fill(self: &mut Account, fill: &Fill) {
        let settled_balances = fill.get_settled_maker_quantities();
        self.settled_balances.add_balances(settled_balances);
        if (!fill.expired()) {
            self.maker_volume = self.maker_volume + fill.base_quantity();
        };
        if (fill.expired() || fill.completed()) {
            self.open_orders.remove(&fill.maker_order_id());
        }
    }

    public(package) fun add_taker_volume(self: &mut Account, volume: u64) {
        self.taker_volume = self.taker_volume + volume;
    }

    /// Set the voted proposal for the account and return the
    /// previous proposal.
    public(package) fun set_voted_proposal(self: &mut Account, proposal: Option<ID>): Option<ID> {
        let prev_proposal = self.voted_proposal;
        self.voted_proposal = proposal;

        prev_proposal
    }

    public(package) fun add_settled_balances(self: &mut Account, balances: Balances) {
        self.settled_balances.add_balances(balances);
    }

    public(package) fun add_owed_balances(self: &mut Account, balances: Balances) {
        self.owed_balances.add_balances(balances);
    }

    /// Settle the account balances. Returns the settled and
    /// owed balances by this account. Vault uses these values
    /// to perform any necessary transfers.
    public(package) fun settle(self: &mut Account): (Balances, Balances) {
        let settled = self.settled_balances.reset();
        let owed = self.owed_balances.reset();

        (settled, owed)
    }

    public(package) fun add_rebates(self: &mut Account, rebates: Balances) {
        self.unclaimed_rebates.add_balances(rebates);
    }

    public(package) fun claim_rebates(self: &mut Account) {
        self.settled_balances.add_balances(self.unclaimed_rebates);
        self.unclaimed_rebates.reset();
    }

    public(package) fun add_order(self: &mut Account, order_id: u128) {
        self.open_orders.insert(order_id);
    }

    public(package) fun remove_order(self: &mut Account, order_id: u128) {
        self.open_orders.remove(&order_id)
    }

    public(package) fun add_stake(self: &mut Account, stake: u64): (u64, u64) {
        let stake_before = self.active_stake + self.inactive_stake;
        self.inactive_stake = self.inactive_stake + stake;
        self.owed_balances.add_deep(stake);

        (stake_before, self.active_stake + self.inactive_stake)
    }

    public(package) fun remove_stake(self: &mut Account) {
        let stake_before = self.active_stake + self.inactive_stake;
        self.active_stake = 0;
        self.inactive_stake = 0;
        self.voted_proposal = option::none();
        self.settled_balances.add_deep(stake_before);
    }

    // === Test Functions ===
    #[test_only]
    public fun settled_balances(self: &Account): Balances {
        self.settled_balances
    }

    #[test_only]
    public fun owed_balances(self: &Account): Balances {
        self.owed_balances
    }
}
