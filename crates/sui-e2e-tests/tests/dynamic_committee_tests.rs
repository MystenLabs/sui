// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use move_core_types::ident_str;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use sui_core::authority::AuthorityState;
use sui_macros::*;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    object::{Object, Owner},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::ObjectStore,
    sui_system_state::{
        sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary},
        SuiSystemStateTrait,
    },
    transaction::{Argument, Command, ObjectArg, ProgrammableTransaction},
    SUI_SYSTEM_PACKAGE_ID,
};
use test_cluster::{TestCluster, TestClusterBuilder};
use tracing::info;

const MAX_DELEGATION_AMOUNT: u64 = 10_000_000_000;
const MIN_DELEGATION_AMOUNT: u64 = 1_000_000_000;

macro_rules! move_call {
    {$builder:expr, ($addr:expr)::$module_name:ident::$func:ident($($args:expr),* $(,)?)} => {
        $builder.programmable_move_call(
            $addr,
            ident_str!(stringify!($module_name)).to_owned(),
            ident_str!(stringify!($func)).to_owned(),
            vec![],
            vec![$($args),*],
        )
    }
}

trait GenStateChange {
    type StateChange: StatePredicate;
    fn create(&self, runner: &mut StressTestRunner) -> Self::StateChange;
}

#[async_trait]
trait StatePredicate {
    async fn run(&mut self, runner: &mut StressTestRunner) -> Result<TransactionEffects>;
    async fn pre_epoch_post_condition(
        &mut self,
        runner: &StressTestRunner,
        effects: &TransactionEffects,
    );
    #[allow(unused)]
    async fn post_epoch_post_condition(
        &mut self,
        runner: &StressTestRunner,
        effects: &TransactionEffects,
    );
}

#[allow(dead_code)]
struct StressTestRunner {
    pub post_epoch_predicates: Vec<Box<dyn StatePredicate + Send + Sync>>,
    pub test_cluster: TestCluster,
    pub accounts: Vec<SuiAddress>,
    pub active_validators: BTreeSet<SuiAddress>,
    pub preactive_validators: BTreeMap<SuiAddress, u64>,
    pub removed_validators: BTreeSet<SuiAddress>,
    pub delegation_requests_this_epoch: BTreeMap<ObjectID, SuiAddress>,
    pub delegation_withdraws_this_epoch: u64,
    pub delegations: BTreeMap<ObjectID, SuiAddress>,
    pub reports: BTreeMap<SuiAddress, BTreeSet<SuiAddress>>,
    pub rng: StdRng,
}

impl StressTestRunner {
    pub async fn new() -> Self {
        let test_cluster = TestClusterBuilder::new()
            .with_accounts(vec![
                AccountConfig {
                    gas_amounts: vec![DEFAULT_GAS_AMOUNT],
                    address: None,
                };
                100
            ])
            .build()
            .await;
        let accounts = test_cluster.wallet.get_addresses();
        Self {
            post_epoch_predicates: vec![],
            test_cluster,
            accounts,
            active_validators: BTreeSet::new(),
            preactive_validators: BTreeMap::new(),
            removed_validators: BTreeSet::new(),
            delegation_requests_this_epoch: BTreeMap::new(),
            delegation_withdraws_this_epoch: 0,
            delegations: BTreeMap::new(),
            reports: BTreeMap::new(),
            rng: StdRng::from_seed([0; 32]),
        }
    }

    pub fn pick_random_sender(&mut self) -> SuiAddress {
        self.accounts[self.rng.gen_range(0..self.accounts.len())]
    }

    pub fn system_state(&self) -> SuiSystemStateSummary {
        self.state()
            .get_sui_system_state_object_for_testing()
            .unwrap()
            .into_sui_system_state_summary()
    }

    pub fn pick_random_active_validator(&mut self) -> SuiValidatorSummary {
        let system_state = self.system_state();
        system_state
            .active_validators
            .get(self.rng.gen_range(0..system_state.active_validators.len()))
            .unwrap()
            .clone()
    }

    pub async fn run(&self, sender: SuiAddress, pt: ProgrammableTransaction) -> TransactionEffects {
        let rgp = self.test_cluster.get_reference_gas_price().await;
        let gas_object = self
            .test_cluster
            .wallet
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap();
        let transaction = self.test_cluster.wallet.sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, rgp)
                .programmable(pt)
                .build(),
        );
        let (effects, _) = self
            .test_cluster
            .execute_transaction_return_raw_effects(transaction)
            .await
            .unwrap();

        assert!(effects.status().is_ok());
        effects
    }

    // Useful for debugging and the like
    pub fn display_effects(&self, effects: &TransactionEffects) {
        println!("CREATED:");
        let state = self.state();

        let epoch_store = state.load_epoch_store_one_call_per_task();
        let backing_package_store = state.get_backing_package_store();
        let mut layout_resolver = epoch_store
            .executor()
            .type_layout_resolver(Box::new(backing_package_store.as_ref()));
        for (obj_ref, _) in effects.created() {
            let object_opt = state
                .get_object_store()
                .get_object_by_key(&obj_ref.0, obj_ref.1);
            let Some(object) = object_opt else { continue };
            let struct_tag = object.struct_tag().unwrap();
            let total_sui =
                object.get_total_sui(layout_resolver.as_mut()).unwrap() - object.storage_rebate;
            println!(">> {struct_tag} TOTAL_SUI: {total_sui}");
        }

        println!("MUTATED:");
        for (obj_ref, _) in effects.mutated() {
            let object = state
                .get_object_store()
                .get_object_by_key(&obj_ref.0, obj_ref.1)
                .unwrap();
            let struct_tag = object.struct_tag().unwrap();
            let total_sui =
                object.get_total_sui(layout_resolver.as_mut()).unwrap() - object.storage_rebate;
            println!(">> {struct_tag} TOTAL_SUI: {total_sui}");
        }

        println!("SHARED:");
        for kind in effects.input_shared_objects() {
            let (obj_id, version) = kind.id_and_version();
            let object = state
                .get_object_store()
                .get_object_by_key(&obj_id, version)
                .unwrap();
            let struct_tag = object.struct_tag().unwrap();
            let total_sui =
                object.get_total_sui(layout_resolver.as_mut()).unwrap() - object.storage_rebate;
            println!(">> {struct_tag} TOTAL_SUI: {total_sui}");
        }
    }

    /*
    pub fn db(&self) -> Arc<AuthorityStore> {
        self.state().db()
    }*/

    pub fn state(&self) -> Arc<AuthorityState> {
        self.test_cluster.fullnode_handle.sui_node.state()
    }

    pub async fn change_epoch(&self) {
        let pre_state_summary = self.system_state();
        self.test_cluster.trigger_reconfiguration().await;
        let post_state_summary = self.system_state();
        info!(
            "Changing epoch form {} to {}",
            pre_state_summary.epoch, post_state_summary.epoch
        );
    }

    pub async fn get_created_object_of_type_name(
        &self,
        effects: &TransactionEffects,
        name: &str,
    ) -> Option<Object> {
        self.get_from_effects(&effects.created(), name).await
    }

    #[allow(dead_code)]
    pub async fn get_mutated_object_of_type_name(
        &self,
        effects: &TransactionEffects,
        name: &str,
    ) -> Option<Object> {
        self.get_from_effects(&effects.mutated(), name).await
    }

    fn split_off(builder: &mut ProgrammableTransactionBuilder, amount: u64) -> Argument {
        let amt_arg = builder.pure(amount).unwrap();
        builder.command(Command::SplitCoins(Argument::GasCoin, vec![amt_arg]))
    }

    async fn get_from_effects(&self, effects: &[(ObjectRef, Owner)], name: &str) -> Option<Object> {
        let db = self.state().get_object_store().clone();
        let found: Vec<_> = effects
            .iter()
            .filter_map(|(obj_ref, _)| {
                let object = db.get_object_by_key(&obj_ref.0, obj_ref.1).unwrap();
                let struct_tag = object.struct_tag().unwrap();
                if struct_tag.name.to_string() == name {
                    Some(object)
                } else {
                    None
                }
            })
            .collect();
        assert!(found.len() <= 1, "Multiple objects of type {name} found");
        found.first().cloned()
    }
}

mod add_stake {
    use super::*;
    use sui_types::effects::TransactionEffects;

    pub struct RequestAddStakeGen;

    pub struct RequestAddStake {
        sender: SuiAddress,
        stake_amount: u64,
        staked_with: SuiAddress,
    }

    impl GenStateChange for RequestAddStakeGen {
        type StateChange = RequestAddStake;

        fn create(&self, runner: &mut StressTestRunner) -> Self::StateChange {
            let stake_amount = runner
                .rng
                .gen_range(MIN_DELEGATION_AMOUNT..=MAX_DELEGATION_AMOUNT);
            let staked_with = runner.pick_random_active_validator().sui_address;
            let sender = runner.pick_random_sender();
            RequestAddStake {
                sender,
                stake_amount,
                staked_with,
            }
        }
    }

    #[async_trait]
    impl StatePredicate for RequestAddStake {
        async fn run(&mut self, runner: &mut StressTestRunner) -> Result<TransactionEffects> {
            let pt = {
                let mut builder = ProgrammableTransactionBuilder::new();
                builder.obj(ObjectArg::SUI_SYSTEM_MUT).unwrap();
                builder.pure(self.staked_with).unwrap();
                let coin = StressTestRunner::split_off(&mut builder, self.stake_amount);
                move_call! {
                    builder,
                    (SUI_SYSTEM_PACKAGE_ID)::sui_system::request_add_stake(Argument::Input(0), coin, Argument::Input(1))
                };
                builder.finish()
            };
            let effects = runner.run(self.sender, pt).await;

            Ok(effects)
        }

        async fn pre_epoch_post_condition(
            &mut self,
            runner: &StressTestRunner,
            effects: &TransactionEffects,
        ) {
            // Assert that a `StakedSui` object matching the amount delegated is created.
            // Assert that this staked sui
            let object = runner
                .get_created_object_of_type_name(effects, "StakedSui")
                .await
                .unwrap();
            let state = runner.state();
            let cache = state.get_backing_package_store();
            let epoch_store = state.load_epoch_store_one_call_per_task();
            let mut layout_resolver = epoch_store
                .executor()
                .type_layout_resolver(Box::new(cache.as_ref()));
            let staked_amount =
                object.get_total_sui(layout_resolver.as_mut()).unwrap() - object.storage_rebate;
            assert_eq!(staked_amount, self.stake_amount);
            assert_eq!(object.owner.get_owner_address().unwrap(), self.sender);
            runner.display_effects(effects);
        }

        async fn post_epoch_post_condition(
            &mut self,
            _runner: &StressTestRunner,
            _effects: &TransactionEffects,
        ) {
            todo!()
        }
    }
}

#[sim_test]
async fn fuzz_dynamic_committee() {
    let num_operations = 10;

    // Add more actions here as we create them
    let actions = [Box::new(add_stake::RequestAddStakeGen)];

    let mut runner = StressTestRunner::new().await;

    for i in 0..num_operations {
        if i == 5 {
            runner.change_epoch().await;
            continue;
        }
        let index = runner.rng.gen_range(0..actions.len());
        let mut task = actions[index].create(&mut runner);
        let effects = task.run(&mut runner).await.unwrap();
        task.pre_epoch_post_condition(&runner, &effects).await;
    }
}
