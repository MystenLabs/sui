// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    workload::{Workload, WorkloadBuilder, MAX_GAS_FOR_TESTING},
    WorkloadBuilderInfo, WorkloadParams,
};
use crate::in_memory_wallet::move_call_pt_impl;
use crate::in_memory_wallet::InMemoryWallet;
use crate::system_state_observer::{SystemState, SystemStateObserver};
use crate::workloads::payload::Payload;
use crate::workloads::{Gas, GasCoinConfig};
use crate::ProgrammableTransactionBuilder;
use crate::{convert_move_call_args, BenchMoveCallArg, ExecutionEffects, ValidatorProxy};
use anyhow::anyhow;
use async_trait::async_trait;
use itertools::Itertools;
use move_core_types::identifier::Identifier;
use rand::distributions::{Distribution, Standard};
use rand::Rng;
use regex::Regex;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount as EnumCountMacro, EnumIter};
use sui_protocol_config::ProtocolConfig;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::messages::Command;
use sui_types::messages::{CallArg, ObjectArg};
use sui_types::{base_types::ObjectID, object::Owner};
use sui_types::{base_types::SuiAddress, crypto::get_key_pair, messages::VerifiedTransaction};
use sui_types::{
    base_types::{random_object_ref, ObjectRef},
    messages::TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
};
use sui_types::{messages::TransactionData, utils::to_sender_signed_transaction};
use tracing::debug;

use test_utils::messages::create_publish_move_package_transaction;
/// Number of vectors to create in LargeTransientRuntimeVectors workload
const NUM_VECTORS: u64 = 1_000;

// TODO: Need to fix Large* workloads, which are currently failing due to InsufficientGas
#[derive(Debug, EnumCountMacro, EnumIter, Clone)]
pub enum AdversarialPayloadType {
    Random = 0,
    LargeObjects,
    LargeEvents,
    DynamicFieldReads,
    LargeTransientRuntimeVectors,
    LargePureFunctionArgs,
    // Creates a bunch of shared objects in the module init for adversarial, then taking them all as input)
    MaxReads,
    // Creates a the largest package publish possible
    MaxPackagePublish,
    // TODO:
    // - MaxReads (by creating a bunch of shared objects in the module init for adversarial, then taking them all as input)
    // - MaxEffects (by creating a bunch of small objects) and mutating lots of objects
    // - MaxCommands (by created the maximum number of PT commands)
    // - MaxTxSize
    //  ...
}
impl Copy for AdversarialPayloadType {}

impl TryFrom<u32> for AdversarialPayloadType {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(rand::random()),
            _ => AdversarialPayloadType::iter()
                .nth(value as usize)
                .ok_or_else(|| {
                    anyhow!(
                        "Invalid adversarial workload specifier. Valid options are {} to {}",
                        0,
                        AdversarialPayloadType::COUNT
                    )
                }),
        }
    }
}

impl FromStr for AdversarialPayloadType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = u32::from_str(s).map(AdversarialPayloadType::try_from);

        if let Ok(Ok(q)) = v {
            return Ok(q);
        }

        Err(anyhow!(
            "Invalid input string. Valid values are 0 to {}",
            AdversarialPayloadType::COUNT
        ))
    }
}

impl Distribution<AdversarialPayloadType> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> AdversarialPayloadType {
        // Exclude the "Random" variant
        let n = rng.gen_range(1..AdversarialPayloadType::COUNT);
        AdversarialPayloadType::iter().nth(n).unwrap()
    }
}

#[derive(Debug)]
pub struct AdversarialTestPayload {
    /// ID of the Move package with adversarial utility functions
    package_id: ObjectID,
    df_parent_obj_ref: ObjectRef,
    /// address to send adversarial transactions from
    sender: SuiAddress,
    /// Shared object refs for checking max reads with contention
    shared_objs: Vec<BenchMoveCallArg>,
    state: InMemoryWallet,
    system_state_observer: Arc<SystemStateObserver>,
    adversarial_payload_cfg: AdversarialPayloadCfg,
}

impl std::fmt::Display for AdversarialTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "adversarial")
    }
}

#[derive(Debug, Clone)]
pub struct AdversarialPayloadCfg {
    pub payload_type: AdversarialPayloadType,
    pub load_factor: f32,
}

impl FromStr for AdversarialPayloadCfg {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Matches regex for two numbers delimited by a hyphen, where the left number must be positive
        // and the right number must be a float between 0.0 inclusive and 1.0 inclusive
        let re = Regex::new(
            r"^(?:0|[1-9]\d*)-(?:0(?:\.\d+)?|1(?:\.0+)?|[1-9](?:\d*(?:\.\d+)?)?|\.\d+)$",
        )
        .unwrap();
        if !re.is_match(s) {
            return Err(anyhow!("invalid load config"));
        };
        let toks = s.split('-').collect_vec();
        let payload_type = AdversarialPayloadType::from_str(toks[0])?;
        let load_factor = toks[1].parse::<f32>().unwrap();

        if !(0.0..=1.0).contains(&load_factor) {
            return Err(anyhow!("invalid load factor. Valid range is [0.0, 1.0]"));
        };

        Ok(AdversarialPayloadCfg {
            payload_type,
            load_factor,
        })
    }
}
impl Copy for AdversarialPayloadCfg {}

impl Payload for AdversarialTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        // Sometimes useful when figuring out why things failed
        let stat = match effects {
            ExecutionEffects::CertifiedTransactionEffects(e, _) => e.data().status(),
            ExecutionEffects::SuiTransactionBlockEffects(_) => unimplemented!("Not impl"),
        };

        debug_assert!(
            effects.is_ok(),
            "Adversarial transactions should never abort: {:?}",
            stat
        );

        self.state.update(effects);
    }

    fn make_transaction(&mut self) -> VerifiedTransaction {
        let payload_type = self.adversarial_payload_cfg.payload_type;

        self.create_transaction(
            &payload_type,
            self.system_state_observer
                .state
                .borrow()
                .protocol_config
                .as_ref()
                .expect("Protocol config not in system state"),
        )
    }
}

impl AdversarialTestPayload {
    // Return a percentage based on load_factor
    fn get_pct_of(&self, x: u64) -> u64 {
        let mut x = x as f64;
        x *= f64::from(self.adversarial_payload_cfg.load_factor);
        x as u64
    }

    fn create_transaction(
        &self,
        payload_type: &AdversarialPayloadType,
        protocol_config: &ProtocolConfig,
    ) -> VerifiedTransaction {
        let args = self.get_payload_args(payload_type, protocol_config);
        let module_name = "adversarial";
        let account = self.state.account(&self.sender).unwrap();
        let gas_budget = protocol_config.max_tx_gas();
        let gas_price = self
            .system_state_observer
            .state
            .borrow()
            .reference_gas_price;
        match payload_type {
            AdversarialPayloadType::MaxReads => {
                let mut builder = ProgrammableTransactionBuilder::new();

                let num_objs_to_read =
                    // We subtract one here because gas counts as one input object
                    self.get_pct_of(self.shared_objs.len() as u64) as usize;
                convert_move_call_args(&self.shared_objs[..num_objs_to_read], &mut builder);

                builder.command(Command::move_call(
                    self.package_id,
                    Identifier::new(module_name).unwrap(),
                    Identifier::new(args.fn_name.as_str()).unwrap(),
                    vec![],
                    vec![],
                ));
                let data = TransactionData::new_programmable(
                    self.sender,
                    vec![account.gas],
                    builder.finish(),
                    gas_budget,
                    gas_price,
                );
                to_sender_signed_transaction(data, account.key())
            }
            AdversarialPayloadType::MaxPackagePublish => {
                let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                path.push("src/workloads/data/max_package");

                create_publish_move_package_transaction(
                    account.gas,
                    path,
                    self.sender,
                    account.key(),
                    gas_price * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
                    gas_price,
                )
            }
            _ => self.state.move_call_pt(
                self.sender,
                self.package_id,
                module_name,
                &args.fn_name,
                vec![],
                args.args,
                gas_budget,
                gas_price,
            ),
        }
    }

    fn get_payload_args(
        &self,
        payload_type: &AdversarialPayloadType,
        protocol_config: &ProtocolConfig,
    ) -> AdversarialPayloadArgs {
        match payload_type {
            AdversarialPayloadType::LargeObjects => AdversarialPayloadArgs {
                fn_name: "create_max_size_shared_objects".to_owned(),
                args: [
                    // Use the maximum number of new ids which can be created
                    self.get_pct_of(protocol_config.max_num_new_move_object_ids())
                        .into(),
                    // Raise this. Using a smaller value here as full value locks up local machine
                    protocol_config.max_move_object_size().into(),
                ]
                .to_vec(),
            },
            AdversarialPayloadType::LargeEvents => AdversarialPayloadArgs {
                fn_name: "emit_events".to_owned(),
                args: [
                    protocol_config.max_num_event_emit().into(),
                    self.get_pct_of(protocol_config.max_event_emit_size())
                        .into(),
                ]
                .to_vec(),
            },
            AdversarialPayloadType::DynamicFieldReads => AdversarialPayloadArgs {
                fn_name: "read_n_dynamic_fields".to_owned(),
                args: [
                    CallArg::Object(ObjectArg::SharedObject {
                        id: self.df_parent_obj_ref.0,
                        initial_shared_version: self.df_parent_obj_ref.1,
                        mutable: true,
                    })
                    .into(),
                    self.get_pct_of(protocol_config.object_runtime_max_num_store_entries())
                        .into(),
                ]
                .to_vec(),
            },
            AdversarialPayloadType::LargeTransientRuntimeVectors => AdversarialPayloadArgs {
                fn_name: "create_vectors_with_size".to_owned(),
                args: [
                    NUM_VECTORS.into(),
                    self.get_pct_of(protocol_config.max_move_vector_len())
                        .into(),
                ]
                .to_vec(),
            },
            AdversarialPayloadType::LargePureFunctionArgs => {
                let max_fn_params = protocol_config.max_function_parameters();
                let max_pure_arg_size =
                    self.get_pct_of(protocol_config.max_pure_argument_size().into());
                let mut args: Vec<BenchMoveCallArg> = vec![];
                (0..max_fn_params).for_each(|_| {
                    let mut v = vec![0u8; max_pure_arg_size as usize];
                    while bcs::to_bytes(&v).unwrap().len() >= max_pure_arg_size as usize {
                        v.pop();
                    }
                    args.push((&v).into());
                });
                AdversarialPayloadArgs {
                    fn_name: "lots_of_params".to_owned(),
                    args,
                }
            }
            AdversarialPayloadType::Random => {
                self.get_payload_args(&(rand::random()), protocol_config)
            }
            AdversarialPayloadType::MaxReads => AdversarialPayloadArgs {
                fn_name: "do_nothing".to_owned(),
                args: vec![],
            },
            AdversarialPayloadType::MaxPackagePublish => AdversarialPayloadArgs {
                // This is a publish so no args needed here
                fn_name: "".to_owned(),
                args: vec![],
            },
        }
    }
}

#[derive(Debug)]
pub struct AdversarialWorkloadBuilder {
    num_payloads: u64,
    adversarial_payload_cfg: AdversarialPayloadCfg,
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for AdversarialWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        // Gas coin for publishing adversarial package
        let (address, keypair) = get_key_pair();
        vec![GasCoinConfig {
            amount: MAX_GAS_FOR_TESTING,
            address,
            keypair: Arc::new(keypair),
        }]
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut configs = vec![];
        // Gas coins for running workload
        for _i in 0..self.num_payloads {
            let (address, keypair) = get_key_pair();
            configs.push(GasCoinConfig {
                amount: MAX_GAS_FOR_TESTING,
                address,
                keypair: Arc::new(keypair),
            });
        }
        configs
    }

    async fn build(
        &self,
        mut init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        debug!(
            "Using `{:?}` adversarial workloads at {}% load factor",
            self.adversarial_payload_cfg.payload_type,
            self.adversarial_payload_cfg.load_factor * 100.0
        );

        Box::<dyn Workload<dyn Payload>>::from(Box::new(AdversarialWorkload {
            package_id: ObjectID::ZERO,
            shared_objs: vec![],
            df_parent_obj_ref: {
                let mut f = random_object_ref();
                f.0 = ObjectID::ZERO;
                f
            },
            init_gas: init_gas.pop().unwrap(),
            payload_gas,
            adversarial_payload_cfg: self.adversarial_payload_cfg,
        }))
    }
}

impl AdversarialWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        adversarial_payload_cfg: AdversarialPayloadCfg,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32) as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            None
        } else {
            let workload_params = WorkloadParams {
                target_qps,
                num_workers,
                max_ops,
            };
            let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
                AdversarialWorkloadBuilder {
                    num_payloads: max_ops,
                    adversarial_payload_cfg,
                },
            ));
            let builder_info = WorkloadBuilderInfo {
                workload_params,
                workload_builder,
            };
            Some(builder_info)
        }
    }
}

#[derive(Debug)]
pub struct AdversarialWorkload {
    /// ID of the Move package with adversarial utility functions
    package_id: ObjectID,
    /// ID of the object used for dynamic field opers
    df_parent_obj_ref: ObjectRef,
    /// Shared object refs for checking max reads with contention
    shared_objs: Vec<BenchMoveCallArg>,
    pub init_gas: Gas,
    pub payload_gas: Vec<Gas>,
    pub adversarial_payload_cfg: AdversarialPayloadCfg,
}

#[async_trait]
impl Workload<dyn Payload> for AdversarialWorkload {
    async fn init(
        &mut self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        let gas = &self.init_gas;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("src/workloads/data/adversarial");
        let SystemState {
            reference_gas_price,
            protocol_config,
        } = system_state_observer.state.borrow().clone();
        let protocol_config = protocol_config.unwrap();
        let gas_budget = protocol_config.max_tx_gas();
        let transaction = create_publish_move_package_transaction(
            gas.0,
            path,
            gas.1,
            &gas.2,
            gas_budget,
            reference_gas_price,
        );
        let effects = proxy
            .execute_transaction_block(transaction.into())
            .await
            .unwrap();
        let created = effects.created();
        // should only create the package object, upgrade cap, dynamic field top level obj, and NUM_DYNAMIC_FIELDS df objects. otherwise, there are some object initializers running and we will need to disambiguate
        assert_eq!(
            created.len() as u64,
            3 + protocol_config.object_runtime_max_num_store_entries()
        );
        let package_obj = created
            .iter()
            .find(|o| matches!(o.1, Owner::Immutable))
            .unwrap();

        for o in &created {
            let obj = proxy.get_object(o.0 .0).await.unwrap();
            if let Some(tag) = obj.data.struct_tag() {
                if tag.to_string().contains("::adversarial::Obj") {
                    self.df_parent_obj_ref = o.0;
                }
            }
        }
        assert!(
            self.df_parent_obj_ref.0 != ObjectID::ZERO,
            "Dynamic field parent must be created"
        );
        self.package_id = package_obj.0 .0;

        let gas_ref = proxy
            .get_object(gas.0 .0)
            .await
            .unwrap()
            .compute_object_reference();
        // Pop off two to avoid hitting max input objs limit since gas and package count as two
        let num_shared_objs = protocol_config.max_input_objects() - 2;
        // Create a bunch of sharedobjects which we will use for MaxReads workload
        let transaction = move_call_pt_impl(
            gas.1,
            &gas.2,
            package_obj.0 .0,
            "adversarial",
            "create_min_size_shared_objects",
            vec![],
            vec![num_shared_objs.into()],
            &gas_ref,
            gas_budget,
            reference_gas_price,
        );

        let effects = proxy
            .execute_transaction_block(transaction.into())
            .await
            .unwrap();

        let created = effects.created();
        assert_eq!(created.len() as u64, num_shared_objs);

        // We've seen that the shared objects are indeed created,we store them so we can read them in MaxReads workload
        self.shared_objs = created
            .iter()
            .map(|o| BenchMoveCallArg::Shared((o.0 .0, o.0 .1, false)))
            .collect();
    }

    async fn make_test_payloads(
        &self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let mut payloads = Vec::new();

        for gas in &self.payload_gas {
            payloads.push(AdversarialTestPayload {
                package_id: self.package_id,
                df_parent_obj_ref: self.df_parent_obj_ref,
                sender: gas.1,
                shared_objs: self.shared_objs.clone(),
                state: InMemoryWallet::new(gas),
                system_state_observer: system_state_observer.clone(),
                adversarial_payload_cfg: self.adversarial_payload_cfg,
            })
        }
        payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(Box::new(b)))
            .collect()
    }
}

struct AdversarialPayloadArgs {
    pub fn_name: String,
    pub args: Vec<BenchMoveCallArg>,
}
