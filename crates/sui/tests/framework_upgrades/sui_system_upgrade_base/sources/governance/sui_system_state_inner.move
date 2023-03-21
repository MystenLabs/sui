// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::sui_system_state_inner {
    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use sui::tx_context::TxContext;
    use sui::validator::Validator;
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set::VecSet;
    use sui::bag::Bag;
    use sui::bag;
    use sui::table_vec;
    use std::vector;
    use sui::table;
    use sui::table_vec::TableVec;
    use sui::table::Table;
    use sui::object::ID;
    use sui::validator_wrapper::ValidatorWrapper;

    friend sui::sui_system;

    const BASIS_POINT_DENOMINATOR: u128 = 10000;

    struct SystemParameters has store {
        /// The starting epoch in which various on-chain governance features take effect:
        /// - stake subsidies are paid out
        governance_start_epoch: u64,

        /// The duration of an epoch, in milliseconds.
        epoch_duration_ms: u64,

        /// Any extra fields that's not defined statically.
        extra_fields: Bag,
    }

    struct StakeSubsidy has store {
        /// Balance of SUI set aside for stake subsidies that will be drawn down over time.
        balance: Balance<SUI>,

        /// Count of the number of times stake subsidies have been distributed.
        distribution_counter: u64,

        /// The amount of stake subsidy to be drawn down per distribution.
        /// This amount decays and decreases over time.
        current_distribution_amount: u64,

        /// Number of distributions to occur before the distribution amount decays.
        stake_subsidy_period_length: u64,

        /// The rate at which the distribution amount decays at the end of each
        /// period. Expressed in basis points.
        stake_subsidy_decrease_rate: u16,

        /// Any extra fields that's not defined statically.
        extra_fields: Bag,
    }

    struct ValidatorSet has store {
        /// Total amount of stake from all active validators at the beginning of the epoch.
        total_stake: u64,

        /// The current list of active validators.
        active_validators: vector<Validator>,

        /// List of new validator candidates added during the current epoch.
        /// They will be processed at the end of the epoch.
        pending_active_validators: TableVec<Validator>,

        /// Removal requests from the validators. Each element is an index
        /// pointing to `active_validators`.
        pending_removals: vector<u64>,

        /// Mappings from staking pool's ID to the sui address of a validator.
        staking_pool_mappings: Table<ID, address>,

        /// Mapping from a staking pool ID to the inactive validator that has that pool as its staking pool.
        /// When a validator is deactivated the validator is removed from `active_validators` it
        /// is added to this table so that stakers can continue to withdraw their stake from it.
        inactive_validators: Table<ID, ValidatorWrapper>,

        /// Table storing preactive validators, mapping their addresses to their `Validator ` structs.
        /// When an address calls `request_add_validator_candidate`, they get added to this table and become a preactive
        /// validator.
        /// When the candidate has met the min stake requirement, they can call `request_add_validator` to
        /// officially add them to the active validator set `active_validators` next epoch.
        validator_candidates: Table<address, ValidatorWrapper>,

        /// Table storing the number of epochs during which a validator's stake has been below the low stake threshold.
        at_risk_validators: VecMap<address, u64>,

        /// Any extra fields that's not defined statically.
        extra_fields: Bag,
    }

    /// The top-level object containing all information of the Sui system.
    struct SuiSystemStateInner has store {
        /// The current epoch ID, starting from 0.
        epoch: u64,
        /// The current protocol version, starting from 1.
        protocol_version: u64,
        /// The current version of the system state data structure type.
        /// This is always the same as SuiSystemState.version. Keeping a copy here so that
        /// we know what version it is by inspecting SuiSystemStateInner as well.
        system_state_version: u64,
        /// Contains all information about the validators.
        validators: ValidatorSet,
        /// The storage fund.
        storage_fund: Balance<SUI>,
        /// A list of system config parameters.
        parameters: SystemParameters,
        /// The reference gas price for the current epoch.
        reference_gas_price: u64,
        /// A map storing the records of validator reporting each other.
        /// There is an entry in the map for each validator that has been reported
        /// at least once. The entry VecSet contains all the validators that reported
        /// them. If a validator has never been reported they don't have an entry in this map.
        /// This map persists across epoch: a peer continues being in a reported state until the
        /// reporter doesn't explicitly remove their report.
        /// Note that in case we want to support validator address change in future,
        /// the reports should be based on validator ids
        validator_report_records: VecMap<address, VecSet<address>>,
        /// Schedule of stake subsidies given out each epoch.
        stake_subsidy: StakeSubsidy,

        /// Whether the system is running in a downgraded safe mode due to a non-recoverable bug.
        /// This is set whenever we failed to execute advance_epoch, and ended up executing advance_epoch_safe_mode.
        /// It can be reset once we are able to successfully execute advance_epoch.
        /// MUSTFIX: We need to save pending gas rewards, so that we could redistribute them.
        safe_mode: bool,
        /// Unix timestamp of the current epoch start
        epoch_start_timestamp_ms: u64,
        /// Any extra fields that's not defined statically.
        extra_fields: Bag,
    }

    // ==== functions that can only be called by genesis ====

    /// Create a new SuiSystemState object and make it shared.
    /// This function will be called only once in genesis.
    public(friend) fun create(
        validators: vector<Validator>,
        stake_subsidy_fund: Balance<SUI>,
        storage_fund: Balance<SUI>,
        protocol_version: u64,
        system_state_version: u64,
        governance_start_epoch: u64,
        epoch_start_timestamp_ms: u64,
        epoch_duration_ms: u64,
        initial_stake_subsidy_distribution_amount: u64,
        stake_subsidy_period_length: u64,
        stake_subsidy_decrease_rate: u16,
        ctx: &mut TxContext,
    ): SuiSystemStateInner {
        let validators = new_validator_set(validators, ctx);
        let system_state = SuiSystemStateInner {
            epoch: 0,
            protocol_version,
            system_state_version,
            validators,
            storage_fund,
            parameters: SystemParameters {
                governance_start_epoch,
                epoch_duration_ms,
                extra_fields: bag::new(ctx),
            },
            reference_gas_price: 1,
            validator_report_records: vec_map::empty(),
            stake_subsidy: create_stake_subsidy(
                stake_subsidy_fund,
                initial_stake_subsidy_distribution_amount,
                stake_subsidy_period_length,
                stake_subsidy_decrease_rate,
                ctx
            ),
            safe_mode: false,
            epoch_start_timestamp_ms,
            extra_fields: bag::new(ctx),
        };
        system_state
    }

    public(friend) fun advance_epoch(
        self: &mut SuiSystemStateInner,
        new_epoch: u64,
        next_protocol_version: u64,
        storage_reward: Balance<SUI>,
        computation_reward: Balance<SUI>,
        storage_rebate_amount: u64,
        epoch_start_timestamp_ms: u64,
    ) : Balance<SUI> {
        self.epoch_start_timestamp_ms = epoch_start_timestamp_ms;
        self.epoch = self.epoch + 1;
        // Sanity check to make sure we are advancing to the right epoch.
        assert!(new_epoch == self.epoch, 0);
        self.safe_mode = false;
        self.protocol_version = next_protocol_version;

        balance::join(&mut self.storage_fund, computation_reward);
        balance::join(&mut self.storage_fund, storage_reward);
        let storage_rebate = balance::split(&mut self.storage_fund, storage_rebate_amount);
        storage_rebate
    }

    public(friend) fun upgrade_system_state(
        self: SuiSystemStateInner,
        new_system_state_version: u64,
        _ctx: &mut TxContext,
    ): SuiSystemStateInner {
        // Whenever we upgrade the system state version, we will have to first
        // ship a framework upgrade that introduces a new system state type, and make this
        // function generate such type from the old state.
        self.system_state_version = new_system_state_version;
        self
    }

    public(friend) fun protocol_version(self: &SuiSystemStateInner): u64 { self.protocol_version }
    public(friend) fun system_state_version(self: &SuiSystemStateInner): u64 { self.system_state_version }

    fun create_stake_subsidy(
        balance: Balance<SUI>,
        initial_stake_subsidy_amount: u64,
        stake_subsidy_period_length: u64,
        stake_subsidy_decrease_rate: u16,
        ctx: &mut TxContext,
    ): StakeSubsidy {
        // Rate can't be higher than 100%.
        assert!(
            stake_subsidy_decrease_rate <= (BASIS_POINT_DENOMINATOR as u16),
            0,
        );

        StakeSubsidy {
            balance,
            distribution_counter: 0,
            current_distribution_amount: initial_stake_subsidy_amount,
            stake_subsidy_period_length,
            stake_subsidy_decrease_rate,
            extra_fields: bag::new(ctx),
        }
    }

    fun new_validator_set(init_active_validators: vector<Validator>, ctx: &mut TxContext): ValidatorSet {
        ValidatorSet {
            total_stake: 0, // total_stake should not matter to run a bare-minimum protocol
            active_validators: init_active_validators,
            pending_active_validators: table_vec::empty(ctx),
            pending_removals: vector::empty(),
            staking_pool_mappings: table::new(ctx),
            inactive_validators: table::new(ctx),
            validator_candidates: table::new(ctx),
            at_risk_validators: vec_map::empty(),
            extra_fields: bag::new(ctx),
        }
    }
}
