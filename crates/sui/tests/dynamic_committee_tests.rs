// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::ed25519::Ed25519KeyPair;
use indexmap::IndexMap;
use move_core_types::ident_str;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use sui_core::authority::AuthorityStore;
use sui_node::SuiNodeHandle;

use sui_config::builder::ConfigBuilder;
use sui_config::{NetworkConfig, NodeConfig};
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorV1;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::{get_key_pair, get_key_pair_from_rng, AccountKeyPair, KeypairTraits},
    messages::{
        Argument, Command, ObjectArg, ProgrammableTransaction, TransactionData, TransactionEffects,
        TransactionEffectsAPI,
    },
    object::{Object, Owner},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::ObjectStore,
    sui_system_state::{
        sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary},
        SuiSystemStateTrait,
    },
    utils::to_sender_signed_transaction,
    SUI_SYSTEM_PACKAGE_ID, SUI_SYSTEM_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};
use test_utils::authority::spawn_test_authorities;
use test_utils::network::{execute_transaction_block, trigger_reconfiguration};
use tracing::info;

const INIT_COMMITTEE_SIZE: usize = 4;
const NUM_POTENTIAL_CANDIDATES: usize = 10;
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
    fn create(&self, runner: &mut StressTestRunner) -> Option<Box<dyn StatePredicate>>;
}

#[async_trait]
trait StatePredicate {
    async fn run(&mut self, runner: &mut StressTestRunner) -> Result<TransactionEffects>;
    async fn pre_epoch_post_condition(
        &mut self,
        runner: &mut StressTestRunner,
        effects: &TransactionEffects,
    );
    async fn post_epoch_post_condition(
        &mut self,
        runner: &mut StressTestRunner,
        effects: &TransactionEffects,
    );
}

#[async_trait]
impl<T: StatePredicate + std::marker::Send> StatePredicate for Box<T> {
    async fn run(&mut self, runner: &mut StressTestRunner) -> Result<TransactionEffects> {
        self.run(runner).await
    }
    async fn pre_epoch_post_condition(
        &mut self,
        runner: &mut StressTestRunner,
        effects: &TransactionEffects,
    ) {
        self.pre_epoch_post_condition(runner, effects).await
    }
    async fn post_epoch_post_condition(
        &mut self,
        runner: &mut StressTestRunner,
        effects: &TransactionEffects,
    ) {
        self.post_epoch_post_condition(runner, effects).await
    }
}

#[allow(dead_code)]
struct StressTestRunner {
    pub post_epoch_predicates: Vec<Box<dyn StatePredicate + Send + Sync>>,
    pub nodes: Vec<SuiNodeHandle>,
    pub accounts: IndexMap<SuiAddress, (AccountKeyPair, ObjectID)>,
    pub potential_candidates: Vec<(ValidatorV1, NodeConfig)>,
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

        // Give enough coins to validators and potential candidates.
        let all_validator_keys = Self::gen_keys(INIT_COMMITTEE_SIZE + NUM_POTENTIAL_CANDIDATES);
        for key in &all_validator_keys {
            let addr = key.public().into();
            let gas_object =
                Object::new_gas_with_balance_and_owner_for_testing(35_000_000_000_000_000, addr);
            accounts.insert(addr, (key.copy(), gas_object.id()));
            objects.push(gas_object);
        }

        for _ in 0..100 {
            let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
            let gas_object_id = ObjectID::random();
            let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
            objects.push(gas_object);
            accounts.insert(sender, (sender_key, gas_object_id));
        }
        let (init_network, potential_candidates) =
            Self::set_up_network_and_potential_candidates(all_validator_keys, objects);
        let nodes = spawn_test_authorities(&init_network).await;
        Self {
            post_epoch_predicates: vec![],
            accounts,
            nodes,
            potential_candidates,
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

    fn set_up_network_and_potential_candidates(
        all_validator_keys: Vec<Ed25519KeyPair>,
        objects: Vec<Object>,
    ) -> (NetworkConfig, Vec<(ValidatorV1, NodeConfig)>) {
        let init_network = ConfigBuilder::new_with_temp_dir()
            .rng(StdRng::from_seed([0; 32]))
            .with_validator_account_keys(Self::gen_keys(INIT_COMMITTEE_SIZE))
            .with_objects(objects.clone())
            .build();
        let init_pubkeys: Vec<_> = init_network
            .genesis
            .validator_set_for_tooling()
            .iter()
            .map(|v| v.verified_metadata().sui_pubkey_bytes())
            .collect();
        let all_validators = ConfigBuilder::new_with_temp_dir()
            .rng(StdRng::from_seed([0; 32]))
            .with_validator_account_keys(all_validator_keys)
            .with_objects(objects.clone())
            .build();
        let potential_candidates: Vec<_> = all_validators
            .genesis
            .validator_set_for_tooling()
            .into_iter()
            .map(|val| {
                let node_config = all_validators
                    .validator_configs()
                    .iter()
                    .find(|config| {
                        config.protocol_public_key() == val.verified_metadata().sui_pubkey_bytes()
                    })
                    .unwrap();
                (val, node_config.clone())
            })
            .filter(|(val, _node)| {
                !init_pubkeys.contains(&val.verified_metadata().sui_pubkey_bytes())
            })
            .collect();
        (init_network, potential_candidates)
    }

    fn gen_keys(count: usize) -> Vec<AccountKeyPair> {
        let mut rng = StdRng::from_seed([0; 32]);
        (0..count)
            .map(|_| get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng).1)
            .collect()
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

        let effects = execute_transaction_block(&self.nodes, signed_txn)
            .await
            .unwrap();
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
        trigger_reconfiguration(&self.nodes).await;
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

    pub struct RequestAddStakeGen;

    pub struct RequestAddStake {
        sender: SuiAddress,
        stake_amount: u64,
        staked_with: SuiAddress,
    }

    impl GenStateChange for RequestAddStakeGen {
        fn create(&self, runner: &mut StressTestRunner) -> Option<Box<dyn StatePredicate>> {
            let stake_amount = runner
                .rng
                .gen_range(MIN_DELEGATION_AMOUNT..=MAX_DELEGATION_AMOUNT);
            let staked_with = runner.pick_random_active_validator().sui_address;
            let sender = runner.pick_random_sender();
            Some(Box::new(RequestAddStake {
                sender,
                stake_amount,
                staked_with,
            }))
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
                    (SUI_SYSTEM_PACKAGE_ID)::sui_system::request_add_stake(Argument::Input(0), coin, Argument::Input(1))
                };
                builder.finish()
            };
            let effects = runner.run(self.sender, pt).await;

            Ok(effects)
        }

        async fn pre_epoch_post_condition(
            &mut self,
            runner: &mut StressTestRunner,
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
            _runner: &mut StressTestRunner,
            _effects: &TransactionEffects,
        ) {
            todo!()
        }
    }
}

mod add_validator_candidate {
    use super::*;
    use fastcrypto::traits::ToFromBytes;
    use sui_types::crypto::generate_proof_of_possession;
    use sui_types::messages::CallArg;
    use sui_types::sui_system_state::sui_system_state_inner_v1::VerifiedValidatorMetadataV1;

    pub struct RequestAddValidatorCandidateGen;

    pub struct RequestAddValidatorCandidate {
        candidate_data: ValidatorV1,
        config: NodeConfig,
    }

    impl GenStateChange for RequestAddValidatorCandidateGen {
        fn create(&self, runner: &mut StressTestRunner) -> Option<Box<dyn StatePredicate>> {
            let num_potential_candidates = runner.potential_candidates.len();
            if num_potential_candidates > 0 {
                let candidate_index = runner.rng.gen_range(0..num_potential_candidates);
                let (candidate_data, config) = runner.potential_candidates.remove(candidate_index);
                Some(Box::new(RequestAddValidatorCandidate {
                    candidate_data,
                    config,
                }))
            } else {
                None
            }
        }
    }

    #[async_trait]
    impl StatePredicate for RequestAddValidatorCandidate {
        async fn run(&mut self, runner: &mut StressTestRunner) -> Result<TransactionEffects> {
            let sender = self.candidate_data.verified_metadata().sui_address;
            let pt = generate_add_validator_candidate_tx(
                &self.config,
                &self.candidate_data.verified_metadata(),
            );
            let effects = runner.run(sender, pt).await;
            Ok(effects)
        }

        async fn pre_epoch_post_condition(
            &mut self,
            runner: &mut StressTestRunner,
            effects: &TransactionEffects,
        ) {
            // todo!()
            runner.display_effects(effects);
        }

        async fn post_epoch_post_condition(
            &mut self,
            _runner: &mut StressTestRunner,
            _effects: &TransactionEffects,
        ) {
            todo!()
        }
    }

    // TODO: maybe share this with reconfiguration_tests
    fn generate_add_validator_candidate_tx(
        node_config: &NodeConfig,
        val: &VerifiedValidatorMetadataV1,
    ) -> ProgrammableTransaction {
        let sender = val.sui_address;
        let proof_of_possession =
            generate_proof_of_possession(node_config.protocol_key_pair(), sender);
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .move_call(
                SUI_SYSTEM_PACKAGE_ID,
                ident_str!("sui_system").to_owned(),
                ident_str!("request_add_validator_candidate").to_owned(),
                vec![],
                vec![
                    CallArg::Object(ObjectArg::SharedObject {
                        id: SUI_SYSTEM_STATE_OBJECT_ID,
                        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                        mutable: true,
                    }),
                    CallArg::Pure(bcs::to_bytes(val.protocol_pubkey.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(val.network_pubkey.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(val.worker_pubkey.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(proof_of_possession.as_ref()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(val.name.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(val.description.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(val.image_url.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(val.project_url.as_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&val.net_address).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&val.p2p_address).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&val.primary_address).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&val.worker_address).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&1u64).unwrap()), // gas_price
                    CallArg::Pure(bcs::to_bytes(&0u64).unwrap()), // commission_rate
                ],
            )
            .expect("fail to creat add validator candidate txn");

        builder.finish()
    }
}

#[tokio::test]
async fn fuzz_dynamic_committee() {
    let num_operations = 10;

    // Add more actions here as we create them
    let actions: Vec<Box<dyn GenStateChange>> = vec![
        Box::new(add_stake::RequestAddStakeGen),
        Box::new(add_validator_candidate::RequestAddValidatorCandidateGen),
    ];

    let mut runner = StressTestRunner::new().await;

    for i in 0..num_operations {
        if i == 5 {
            runner.change_epoch().await;
            continue;
        }
        let index = runner.rng.gen_range(0..actions.len());
        let task = actions[index].create(&mut runner);
        if task.is_none() {
            continue;
        }
        let mut task = task.unwrap();
        let effects = task.run(&mut runner).await.unwrap();
        task.pre_epoch_post_condition(&mut runner, &effects).await;
    }
}
