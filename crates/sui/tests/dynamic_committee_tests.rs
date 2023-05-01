// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use indexmap::IndexMap;
use move_core_types::ident_str;
use prometheus::Registry;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};
use sui_core::{
    authority::AuthorityStore,
    authority_aggregator::{AuthAggMetrics, AuthorityAggregator},
    safe_client::SafeClientMetricsBase,
};
use sui_node::SuiNodeHandle;

use sui_types::effects::{CertifiedTransactionEffects, TransactionEffects, TransactionEffectsAPI};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    messages::{
        Argument, Command, ObjectArg, ProgrammableTransaction, TransactionData, VerifiedTransaction,
    },
    object::{Object, Owner},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::ObjectStore,
    sui_system_state::{
        sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary},
        SuiSystemStateTrait,
    },
    utils::to_sender_signed_transaction,
    SUI_SYSTEM_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};
use test_utils::authority::spawn_test_authorities;
use test_utils::authority::test_authority_configs_with_objects;
use tokio::time::timeout;
use tracing::{info, warn};

const MAX_DELEGATION_AMOUNT: u64 = 10_000_000_000;
const MIN_DELEGATION_AMOUNT: u64 = 1_000_000_000;
const MAX_GAS: u64 = 100_000_000;

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
    async fn post_epoch_post_condition(
        &mut self,
        runner: &StressTestRunner,
        effects: &TransactionEffects,
    );
}

#[allow(dead_code)]
struct StressTestRunner {
    pub post_epoch_predicates: Vec<Box<dyn StatePredicate + Send + Sync>>,
    pub nodes: Vec<SuiNodeHandle>,
    pub accounts: IndexMap<SuiAddress, (AccountKeyPair, ObjectID)>,
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
        // let authority_state = init_state().await;
        let mut accounts = IndexMap::new();
        let mut objects = vec![];
        for _ in 0..100 {
            let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
            let gas_object_id = ObjectID::random();
            let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
            objects.push(gas_object);
            // authority_state.insert_genesis_object(gas_object).await;
            accounts.insert(sender, (sender_key, gas_object_id));
        }
        let (net_config, _) = test_authority_configs_with_objects(objects);
        let nodes = spawn_test_authorities(&net_config).await;
        Self {
            post_epoch_predicates: vec![],
            accounts,
            nodes,
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
        *self
            .accounts
            .get_index(self.rng.gen_range(0..self.accounts.len()))
            .unwrap()
            .0
    }

    pub fn system_state(&self) -> SuiSystemStateSummary {
        self.nodes[0].with(|node| {
            node.state()
                .get_sui_system_state_object_for_testing()
                .unwrap()
                .into_sui_system_state_summary()
        })
    }

    pub fn pick_random_active_validator(&mut self) -> SuiValidatorSummary {
        let system_state = self.system_state();
        system_state
            .active_validators
            .get(self.rng.gen_range(0..system_state.active_validators.len()))
            .unwrap()
            .clone()
    }

    async fn execute_transaction_block(
        &self,
        transaction: VerifiedTransaction,
    ) -> anyhow::Result<CertifiedTransactionEffects> {
        let registry = Registry::new();
        let net = AuthorityAggregator::new_from_local_system_state(
            &self.nodes[0].with(|node| node.state().db()),
            &self.nodes[0].with(|node| node.state().committee_store().clone()),
            SafeClientMetricsBase::new(&registry),
            AuthAggMetrics::new(&registry),
        )
        .unwrap();
        net.execute_transaction_block(&transaction)
            .await
            .map(|e| e.into_inner())
    }

    async fn trigger_reconfiguration(authorities: &[SuiNodeHandle]) {
        info!("Starting reconfiguration");
        let start = Instant::now();

        // Close epoch on 2f+1 validators.
        let cur_committee =
            authorities[0].with(|node| node.state().epoch_store_for_testing().committee().clone());
        let mut cur_stake = 0;
        for handle in authorities {
            handle
                .with_async(|node| async {
                    node.close_epoch_for_testing().await.unwrap();
                    cur_stake += cur_committee.weight(&node.state().name);
                })
                .await;
            if cur_stake >= cur_committee.quorum_threshold() {
                break;
            }
        }
        info!("close_epoch complete after {:?}", start.elapsed());

        // Wait for all nodes to reach the next epoch.
        let handles: Vec<_> = authorities
            .iter()
            .map(|handle| {
                handle.with_async(|node| async {
                    let mut retries = 0;
                    loop {
                        if node.state().epoch_store_for_testing().epoch() == cur_committee.epoch + 1 {
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        retries += 1;
                        if retries % 5 == 0 {
                            warn!(validator=?node.state().name.concise(), "Waiting for {:?} seconds for epoch change", retries);
                        }
                    }
                })
            })
        .collect();

        timeout(Duration::from_secs(40), join_all(handles))
            .await
            .expect("timed out waiting for reconfiguration to complete");

        info!("reconfiguration complete after {:?}", start.elapsed());
    }

    pub async fn run(
        &mut self,
        sender: SuiAddress,
        pt: ProgrammableTransaction,
    ) -> TransactionEffects {
        let (sender_key, gas_object_id) = self.accounts.get(&sender).unwrap();
        let (gas_object_ref, rgp) = self.nodes[0].with(|node| {
            let gas_object = node
                .state()
                .db()
                .get_object(gas_object_id)
                .unwrap()
                .unwrap();
            let rgp = node.reference_gas_price_for_testing().unwrap();
            (gas_object.compute_object_reference(), rgp)
        });
        let signed_txn = to_sender_signed_transaction(
            TransactionData::new_programmable(sender, vec![gas_object_ref], pt, MAX_GAS, rgp),
            sender_key,
        );

        let effects = self.execute_transaction_block(signed_txn).await.unwrap();
        assert!(effects.status().is_ok());
        effects.into_data()
    }

    // Useful for debugging and the like
    pub fn display_effects(&self, effects: &TransactionEffects) {
        let TransactionEffects::V1(effects) = effects;
        println!("CREATED:");
        self.nodes[0].with(|node| {
            let state = node.state();
            for (obj_ref, _) in &effects.created {
                let object_opt = state
                    .database
                    .get_object_by_key(&obj_ref.0, obj_ref.1)
                    .unwrap();
                let Some(object) = object_opt else { continue };
                let struct_tag = object.struct_tag().unwrap();
                let total_sui =
                    object.get_total_sui(&state.database).unwrap() - object.storage_rebate;
                println!(">> {struct_tag} TOTAL_SUI: {total_sui}");
            }

            println!("MUTATED:");
            for (obj_ref, _) in &effects.mutated {
                let object = state
                    .database
                    .get_object_by_key(&obj_ref.0, obj_ref.1)
                    .unwrap()
                    .unwrap();
                let struct_tag = object.struct_tag().unwrap();
                let total_sui =
                    object.get_total_sui(&state.database).unwrap() - object.storage_rebate;
                println!(">> {struct_tag} TOTAL_SUI: {total_sui}");
            }

            println!("SHARED:");
            for (obj_id, version, _) in &effects.shared_objects {
                let object = state
                    .database
                    .get_object_by_key(obj_id, *version)
                    .unwrap()
                    .unwrap();
                let struct_tag = object.struct_tag().unwrap();
                let total_sui =
                    object.get_total_sui(&state.database).unwrap() - object.storage_rebate;
                println!(">> {struct_tag} TOTAL_SUI: {total_sui}");
            }
        })
    }

    pub async fn db(&self) -> Arc<AuthorityStore> {
        self.nodes[0].with(|node| node.state().db())
    }

    pub async fn change_epoch(&mut self) {
        let pre_state_summary = self.system_state();
        Self::trigger_reconfiguration(&self.nodes).await;
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
        let TransactionEffects::V1(effects) = effects;
        self.get_from_effects(&effects.created, name).await
    }

    #[allow(dead_code)]
    pub async fn get_mutated_object_of_type_name(
        &self,
        effects: &TransactionEffects,
        name: &str,
    ) -> Option<Object> {
        let TransactionEffects::V1(effects) = effects;
        self.get_from_effects(&effects.mutated, name).await
    }

    fn split_off(builder: &mut ProgrammableTransactionBuilder, amount: u64) -> Argument {
        let amt_arg = builder.pure(amount).unwrap();
        builder.command(Command::SplitCoins(Argument::GasCoin, vec![amt_arg]))
    }

    async fn get_from_effects(&self, effects: &[(ObjectRef, Owner)], name: &str) -> Option<Object> {
        let db = self.db().await;
        let found: Vec<_> = effects
            .iter()
            .filter_map(|(obj_ref, _)| {
                let object = db
                    .get_object_by_key(&obj_ref.0, obj_ref.1)
                    .unwrap()
                    .unwrap();
                let struct_tag = object.struct_tag().unwrap();
                if struct_tag.name.to_string() == name {
                    Some(object)
                } else {
                    None
                }
            })
            .collect();
        assert!(found.len() <= 1, "Multiple objects of type {name} found");
        found.get(0).cloned()
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
                builder
                    .obj(ObjectArg::SharedObject {
                        id: SUI_SYSTEM_STATE_OBJECT_ID,
                        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                        mutable: true,
                    })
                    .unwrap();
                builder.pure(self.staked_with).unwrap();
                let coin = StressTestRunner::split_off(&mut builder, self.stake_amount);
                move_call! {
                    builder,
                    (SUI_SYSTEM_OBJECT_ID)::sui_system::request_add_stake(Argument::Input(0), coin, Argument::Input(1))
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
            let staked_amount =
                object.get_total_sui(&runner.db().await).unwrap() - object.storage_rebate;
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

#[tokio::test]
async fn fuzz_dynamic_committee() {
    let num_operations = 10;

    // Add more actions here as we create them
    let actions = vec![Box::new(add_stake::RequestAddStakeGen)];

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
