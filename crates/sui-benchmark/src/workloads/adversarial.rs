// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use rand::distributions::{Distribution, Standard};
use rand::Rng;
use std::path::PathBuf;
use std::sync::Arc;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount as EnumCountMacro, EnumIter};
use sui_protocol_config::ProtocolConfig;
use sui_types::messages::CallArg;
use sui_types::{base_types::ObjectID, object::Owner};
use sui_types::{base_types::SuiAddress, crypto::get_key_pair, messages::VerifiedTransaction};
use test_utils::messages::create_publish_move_package_transaction;

use crate::in_memory_wallet::InMemoryWallet;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::{Gas, GasCoinConfig};
use crate::{ExecutionEffects, ValidatorProxy};

use super::{
    workload::{Workload, WorkloadBuilder, MAX_GAS_FOR_TESTING},
    WorkloadBuilderInfo, WorkloadParams,
};

/// Number of max size objects to create in the max object payload
const NUM_OBJECTS: u64 = 2048;

#[derive(Debug, EnumCountMacro, EnumIter)]
enum AdversarialPayloadType {
    ObjectsSize = 1,
    EventSize,
    EventCount,
    DynamicFieldsCount,
    // TODO:
    // - MaxReads (by creating a bunch of shared objects in the module init for adversarial, then taking them all as input)
    // - MaxEffects (by creating a bunch of small objects) and mutating lots of objects
    // - MaxCommands (by created the maximum number of PT commands)
    // ...
}

impl Distribution<AdversarialPayloadType> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> AdversarialPayloadType {
        let n = rng.gen_range(0..AdversarialPayloadType::COUNT);
        AdversarialPayloadType::iter().nth(n).unwrap()
    }
}

#[derive(Debug)]
pub struct AdversarialTestPayload {
    /// ID of the Move package with adversarial utility functions
    package_id: ObjectID,
    /// address to send adversarial transactions from
    sender: SuiAddress,
    state: InMemoryWallet,
    system_state_observer: Arc<SystemStateObserver>,
}

impl std::fmt::Display for AdversarialTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "adversarial")
    }
}

impl Payload for AdversarialTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        // Sometimes useful when figuring out why things failed
        // let stat = match effects {
        //     ExecutionEffects::CertifiedTransactionEffects(e, _) => e.data().status(),
        //     ExecutionEffects::SuiTransactionEffects(_) => unimplemented!("Not impl"),
        // };
        // println!("ERRR: {:?}", stat);

        // important to keep this as a sanity check that we don't hit protocol limits or run out of gas as things change elsewhere.
        // adversarial tests aren't much use if they don't have effects :)
        debug_assert!(
            effects.is_ok(),
            "Adversarial transactions should never abort"
        );

        self.state.update(effects);
    }

    fn make_transaction(&mut self) -> VerifiedTransaction {
        let num_payload_types: AdversarialPayloadType = rand::random();

        let gas_budget = self.system_state_observer.protocol_config.max_tx_gas();
        let payload_args = get_payload_args(
            num_payload_types,
            &self.system_state_observer.protocol_config,
        );

        self.state.move_call(
            self.sender,
            self.package_id,
            "adversarial",
            &payload_args.fn_name,
            vec![],
            payload_args.args,
            gas_budget,
            *self.system_state_observer.reference_gas_price.borrow(),
        )
    }
}

#[derive(Debug)]
pub struct AdversarialWorkloadBuilder {
    num_payloads: u64,
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
        Box::<dyn Workload<dyn Payload>>::from(Box::new(AdversarialWorkload {
            package_id: ObjectID::ZERO,
            init_gas: init_gas.pop().unwrap(),
            payload_gas,
        }))
    }
}

impl AdversarialWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
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
    pub init_gas: Gas,
    pub payload_gas: Vec<Gas>,
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
        let gas_price = *system_state_observer.reference_gas_price.borrow();
        let transaction =
            create_publish_move_package_transaction(gas.0, path, gas.1, &gas.2, Some(gas_price));
        let effects = proxy.execute_transaction(transaction.into()).await.unwrap();
        let created = effects.created();
        // should only create the package object + upgrade cap. otherwise, there are some object initializers running and we will need to disambiguate
        assert_eq!(created.len(), 2);
        let package_obj = created
            .iter()
            .find(|o| matches!(o.1, Owner::Immutable))
            .unwrap();
        self.package_id = package_obj.0 .0;
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
                sender: gas.1,
                state: InMemoryWallet::new(gas),
                system_state_observer: system_state_observer.clone(),
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
    pub args: Vec<CallArg>,
}

fn get_payload_args(
    payload_type: AdversarialPayloadType,
    protocol_config: &ProtocolConfig,
) -> AdversarialPayloadArgs {
    match payload_type {
        AdversarialPayloadType::ObjectsSize => AdversarialPayloadArgs {
            fn_name: "create_shared_objects".to_owned(),
            args: [
                // TODO: Raise this. Using a smaller value here as full value locks up local machine
                (NUM_OBJECTS / 10).into(),
                // TODO: Raise this. Using a smaller value here as full value locks up local machine
                (protocol_config.max_move_object_size() / 10).into(),
            ]
            .to_vec(),
        },
        AdversarialPayloadType::EventSize => AdversarialPayloadArgs {
            fn_name: "emit_events".to_owned(),
            args: [
                // TODO: Raise this. Using a smaller value here as full value locks up local machine
                (protocol_config.max_num_event_emit() / 10).into(),
                // TODO: Raise this. Using a smaller value here as full value errs
                (protocol_config.max_event_emit_size() / 3).into(),
            ]
            .to_vec(),
        },
        AdversarialPayloadType::DynamicFieldsCount => AdversarialPayloadArgs {
            fn_name: "add_dynamic_fields".to_owned(),
            args: [protocol_config
                .object_runtime_max_num_cached_objects()
                .into()]
            .to_vec(),
        },
        AdversarialPayloadType::EventCount => AdversarialPayloadArgs {
            fn_name: "emit_events".to_owned(),
            args: [
                // TODO: Raise this. Using a smaller value here as full value locks up local machine
                protocol_config.max_num_event_emit().into(),
                // Intentionally emitting small objects as we're only checking the count
                (10u64).into(),
            ]
            .to_vec(),
        },
    }
}
