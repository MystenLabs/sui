// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    workload::{Workload, WorkloadBuilder, MAX_GAS_FOR_TESTING},
    WorkloadBuilderInfo, WorkloadParams,
};
use crate::drivers::Interval;
use crate::in_memory_wallet::InMemoryWallet;
use crate::system_state_observer::{SystemState, SystemStateObserver};
use crate::workloads::benchmark_move_base_dir;
use crate::workloads::payload::Payload;
use crate::workloads::{workload::ExpectedFailureType, Gas, GasCoinConfig};
use crate::ProgrammableTransactionBuilder;
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use move_core_types::identifier::Identifier;
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::transaction::ObjectArg;
use sui_types::{base_types::ObjectID, object::Owner};
use sui_types::{base_types::SuiAddress, crypto::get_key_pair, transaction::Transaction};
use sui_types::{
    base_types::{random_object_ref, ObjectRef},
    SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
};

#[derive(Debug)]
pub struct SlowTestPayload {
    /// ID of the Move package with slow utility functions
    package_id: ObjectID,
    shared_object_ref: ObjectRef,
    /// address to send slow transactions from
    sender: SuiAddress,
    state: InMemoryWallet,
    system_state_observer: Arc<SystemStateObserver>,
}

impl std::fmt::Display for SlowTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "slow")
    }
}

impl Payload for SlowTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        debug_assert!(
            effects.is_ok(),
            "Slow transactions should never abort: {effects:?}",
        );

        self.state.update(effects);
    }

    fn make_transaction(&mut self) -> Transaction {
        self.create_transaction()
    }

    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None
    }
}

impl SlowTestPayload {
    fn create_transaction(&self) -> Transaction {
        let account = self.state.account(&self.sender).unwrap();
        let gas_price = self
            .system_state_observer
            .state
            .borrow()
            .reference_gas_price;

        let mut builder = ProgrammableTransactionBuilder::new();
        let args = vec![builder
            .obj(ObjectArg::SharedObject {
                id: SUI_CLOCK_OBJECT_ID,
                initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                mutable: false,
            })
            .unwrap()];
        builder.programmable_move_call(
            self.package_id,
            Identifier::new("slow").unwrap(),
            Identifier::new("bimodal").unwrap(),
            vec![],
            args,
        );

        // Add unused mutable shared object input to activate congestion control.
        builder
            .obj(ObjectArg::SharedObject {
                id: self.shared_object_ref.0,
                initial_shared_version: self.shared_object_ref.1,
                mutable: true,
            })
            .unwrap();

        TestTransactionBuilder::new(self.sender, account.gas, gas_price)
            .programmable(builder.finish())
            .build_and_sign(account.key())
    }
}

#[derive(Debug)]
pub struct SlowWorkloadBuilder {
    num_payloads: u64,
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for SlowWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        // Gas coin for publishing package
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
        Box::<dyn Workload<dyn Payload>>::from(Box::new(SlowWorkload {
            package_id: ObjectID::ZERO,
            shared_obj_ref: {
                let mut f = random_object_ref();
                f.0 = ObjectID::ZERO;
                f
            },
            init_gas: init_gas.pop().unwrap(),
            payload_gas,
        }))
    }
}

impl SlowWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32).ceil() as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            None
        } else {
            let workload_params = WorkloadParams {
                target_qps,
                num_workers,
                max_ops,
                duration,
                group,
            };
            let workload_builder =
                Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(SlowWorkloadBuilder {
                    num_payloads: max_ops,
                }));
            let builder_info = WorkloadBuilderInfo {
                workload_params,
                workload_builder,
            };
            Some(builder_info)
        }
    }
}

#[derive(Debug)]
pub struct SlowWorkload {
    /// ID of the Move package with slow functions
    package_id: ObjectID,
    /// ID of the object used for mutable shared input
    shared_obj_ref: ObjectRef,
    /// Shared object refs for checking max reads with contention
    // shared_objs: Vec<BenchMoveCallArg>,
    pub init_gas: Gas,
    pub payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for SlowWorkload {
    async fn init(
        &mut self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        let gas = &self.init_gas;
        let mut path = benchmark_move_base_dir();
        path.push("src/workloads/data/slow");
        let SystemState {
            reference_gas_price,
            protocol_config: _,
        } = system_state_observer.state.borrow().clone();
        let transaction = TestTransactionBuilder::new(gas.1, gas.0, reference_gas_price)
            .publish(path)
            .build_and_sign(gas.2.as_ref());
        let effects = proxy.execute_transaction_block(transaction).await.unwrap();
        let created = effects.created();
        // should only create the package object, upgrade cap, shared obj.
        assert_eq!(created.len() as u64, 3);
        let package_obj = created
            .iter()
            .find(|o| matches!(o.1, Owner::Immutable))
            .unwrap();

        for o in &created {
            let obj = proxy.get_object(o.0 .0).await.unwrap();
            if let Some(tag) = obj.data.struct_tag() {
                if tag.to_string().contains("::slow::Obj") {
                    self.shared_obj_ref = o.0;
                    break;
                }
            }
        }
        assert!(
            self.shared_obj_ref.0 != ObjectID::ZERO,
            "Dynamic field parent must be created"
        );
        self.package_id = package_obj.0 .0;
    }

    async fn make_test_payloads(
        &self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let mut payloads = Vec::new();

        for gas in &self.payload_gas {
            payloads.push(SlowTestPayload {
                package_id: self.package_id,
                shared_object_ref: self.shared_obj_ref,
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
