// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::drivers::Interval;
use crate::in_memory_wallet::move_call_pt_impl;
use crate::system_state_observer::{SystemState, SystemStateObserver};
use crate::workloads::payload::Payload;
use crate::workloads::workload::{
    Workload, WorkloadBuilder, ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING,
};
use crate::workloads::GasCoinConfig;
use crate::workloads::{Gas, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use move_core_types::language_storage::TypeTag;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use sui_core::test_utils::make_transfer_object_transaction;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::crypto::get_key_pair;
use sui_types::object::Owner;
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    transaction::{CallArg, ObjectArg, Transaction},
    SUI_FRAMEWORK_PACKAGE_ID,
};
use tracing::{error, info};

/// The max amount of gas units needed for a payload.
pub const MAX_GAS_IN_UNIT: u64 = 1_000_000_000;

pub const USDT: &str = "::coins::USDT";
pub const XBTC: &str = "::coins::XBTC";

pub const ONECOIN: usize = 100000000;
pub const POOLCOIN: usize = 1000000000000;
pub const COIN_EACH_OBJ: usize = 10000000;

#[derive(Debug)]
pub struct SammTestPayload {
    samm_package_id: ObjectID,
    coin_package_id: ObjectID,
    xbtc_small_coin_ref: ObjectRef,
    global_arg: CallArg,
    gas: Gas,
    gas_budget: u64,
    system_state_observer: Arc<SystemStateObserver>,
}

impl std::fmt::Display for SammTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "samm")
    }
}

impl Payload for SammTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        if !effects.is_ok() {
            effects.print_gas_summary();
            error!("samm tx failed... Status: {:?}", effects.status());
        }
        self.gas.0 = effects.gas_object().0;

        let mutated = effects.mutated();

        for o in &mutated {
            if o.0 .0 == self.xbtc_small_coin_ref.0 {
                self.xbtc_small_coin_ref = o.0;
                break;
            }
        }
    }
    fn make_transaction(&mut self) -> Transaction {
        let rgp = self
            .system_state_observer
            .state
            .borrow()
            .reference_gas_price;

        let usdt_type = self.coin_package_id.to_string() + USDT;
        let usdt_type_tag = TypeTag::from_str(usdt_type.as_str()).unwrap();
        let xbtc_type = self.coin_package_id.to_string() + XBTC;
        let xbtc_type_tag = TypeTag::from_str(xbtc_type.as_str()).unwrap();
        let xbtc_arg = CallArg::Object(ObjectArg::ImmOrOwnedObject(self.xbtc_small_coin_ref));

        move_call_pt_impl(
            self.gas.1,
            &self.gas.2,
            self.samm_package_id,
            "interface",
            "swap",
            vec![xbtc_type_tag.clone(), usdt_type_tag.clone()],
            vec![
                self.global_arg.clone().into(),
                xbtc_arg.clone().into(),
                (100000 as u64).into(),
            ],
            &self.gas.0,
            self.gas_budget,
            rgp,
        )
    }
}

#[derive(Debug)]
pub struct SammWorkloadBuilder {
    num_payloads: u64,
    rgp: u64,
}

impl SammWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        reference_gas_price: u64,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        info!("creating samm workload builder ...");
        let target_qps = (workload_weight * target_qps as f32) as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            None
        } else {
            let workload_params = WorkloadParams {
                group,
                target_qps,
                num_workers,
                max_ops,
                duration,
            };
            let workload_builder =
                Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(SammWorkloadBuilder {
                    num_payloads: max_ops,
                    rgp: reference_gas_price,
                }));
            let builder_info = WorkloadBuilderInfo {
                workload_params,
                workload_builder,
            };
            Some(builder_info)
        }
    }
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for SammWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        info!("generating coin config...");
        let (address, keypair) = get_key_pair();
        vec![GasCoinConfig {
            amount: MAX_GAS_FOR_TESTING,
            address,
            keypair: Arc::new(keypair),
        }]
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        info!("generate coin config for payloads ...");
        let mut configs = vec![];
        let amount = MAX_GAS_IN_UNIT * self.rgp + ESTIMATED_COMPUTATION_COST;
        // Gas coins for running workload
        for _i in 0..self.num_payloads {
            let (address, keypair) = get_key_pair();
            configs.push(GasCoinConfig {
                amount,
                address,
                keypair: Arc::new(keypair),
            });
        }
        configs
    }

    async fn build(
        &self,
        init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        info!("build samm workload builder ...");
        Box::<dyn Workload<dyn Payload>>::from(Box::new(SammWorkload {
            samm_package_id: None,
            coins_package_id: None,
            global_id: None,
            faucet_id: None,
            xbtc_coin_id: None,
            usdt_coin_id: None,
            large_xbtc_coin_id: None,
            xbtc_small_coin_refs: vec![],
            global_arg: None,
            gas_budget: 0,
            init_gas,
            payload_gas,
        }))
    }
}

#[derive(Debug)]
pub struct SammWorkload {
    pub samm_package_id: Option<ObjectID>,
    pub coins_package_id: Option<ObjectID>,
    pub global_id: Option<ObjectID>,
    pub faucet_id: Option<ObjectID>,
    pub xbtc_coin_id: Option<ObjectID>,
    pub usdt_coin_id: Option<ObjectID>,
    pub large_xbtc_coin_id: Option<ObjectID>,
    pub xbtc_small_coin_refs: Vec<ObjectRef>,
    pub global_arg: Option<CallArg>,
    pub gas_budget: u64,
    pub init_gas: Vec<Gas>,
    pub payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for SammWorkload {
    async fn init(
        &mut self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        info!("init samm workload...");

        // 1. Publish SAMM package
        let gas = self
            .init_gas
            .first()
            .expect("Not enough gas to initialize samm workload");
        let owner_address = gas.1;
        info!("owner address {owner_address:?}");

        let mut path = if let Ok(ptn_path) = std::env::var("PTN_MOVE_DIR") {
            PathBuf::from(ptn_path)
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        };
        path.push("src/workloads/data/samm");
        let SystemState {
            reference_gas_price,
            protocol_config,
        } = system_state_observer.state.borrow().clone();
        let protocol_config = protocol_config.unwrap();
        let _gas_budget = protocol_config.max_tx_gas();
        let transaction = TestTransactionBuilder::new(gas.1, gas.0, reference_gas_price)
            .publish_with_deps(path)
            .build_and_sign(gas.2.as_ref());
        let effects = proxy.execute_transaction_block(transaction).await.unwrap();

        let created = effects.created();

        let samm_package_obj = created
            .iter()
            .find(|o| matches!(o.1, Owner::Immutable))
            .unwrap();

        self.samm_package_id = Some(samm_package_obj.0 .0);
        info!("samm package_id: {:?}", self.samm_package_id);

        let mut global_ref = None;
        for o in &created {
            let obj = proxy.get_object(o.0 .0).await.unwrap();
            if let Some(tag) = obj.data.struct_tag() {
                if tag.name.as_str().starts_with("Global") {
                    global_ref = Some(obj.compute_object_reference());
                    self.global_id = Some(o.0 .0);
                }
            }
        }
        info!("global_id: {:?}", self.global_id);

        // Publish test coins
        let updated_gas_ref = proxy
            .get_object(gas.0 .0)
            .await
            .unwrap()
            .compute_object_reference();
        path = if let Ok(ptn_path) = std::env::var("PTN_MOVE_DIR") {
            PathBuf::from(ptn_path)
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        };
        path.push("src/workloads/data/samm/test_coins");
        let gas_budget = protocol_config.max_tx_gas();
        let transaction = TestTransactionBuilder::new(gas.1, updated_gas_ref, reference_gas_price)
            .publish_with_deps(path)
            .build_and_sign(gas.2.as_ref());
        let effects = proxy.execute_transaction_block(transaction).await.unwrap();

        let created = effects.created();

        let coin_package_obj = created
            .iter()
            .find(|o| matches!(o.1, Owner::Immutable))
            .unwrap();

        self.coins_package_id = Some(coin_package_obj.0 .0);
        info!("coin package_id: {:?}", self.coins_package_id);

        let mut faucet_ref = None;
        for o in &created {
            let obj = proxy.get_object(o.0 .0).await.unwrap();
            if let Some(tag) = obj.data.struct_tag() {
                if tag.name.as_str().starts_with("Faucet") {
                    faucet_ref = Some(obj.compute_object_reference());
                    self.faucet_id = Some(o.0 .0);
                }
            }
        }
        info!("faucet_id: {:?}", self.faucet_id);

        // 3. Add sender as faucet admin
        let updated_gas_ref = proxy
            .get_object(gas.0 .0)
            .await
            .unwrap()
            .compute_object_reference();

        let owner_arg = CallArg::Pure(bcs::to_bytes(&owner_address).unwrap());
        let faucet_ref = faucet_ref.unwrap();
        let faucet_arg = CallArg::Object(ObjectArg::SharedObject {
            id: faucet_ref.0,
            initial_shared_version: faucet_ref.1,
            mutable: true,
        });
        let transaction = move_call_pt_impl(
            gas.1,
            &gas.2,
            coin_package_obj.0 .0,
            "faucet",
            "add_admin",
            vec![],
            vec![faucet_arg.clone().into(), owner_arg.into()],
            &updated_gas_ref,
            gas_budget,
            reference_gas_price,
        );
        let effects = proxy.execute_transaction_block(transaction).await.unwrap();

        if effects.is_ok() {
            info!("sender added as faucet admin");
        } else {
            error!("sender not added as faucet admin {effects:?}");
        }

        // 4. Get coins to put into a liquidity pool
        let updated_gas_ref = proxy
            .get_object(gas.0 .0)
            .await
            .unwrap()
            .compute_object_reference();

        let _coins_from_pool = POOLCOIN / ONECOIN;

        let usdt_type = self.coins_package_id.unwrap().to_string() + USDT;
        let usdt_type_tag = TypeTag::from_str(usdt_type.as_str()).unwrap();
        let xbtc_type = self.coins_package_id.unwrap().to_string() + XBTC;
        let xbtc_type_tag = TypeTag::from_str(xbtc_type.as_str()).unwrap();
        let transaction = move_call_pt_impl(
            gas.1,
            &gas.2,
            coin_package_obj.0 .0,
            "faucet",
            "force_claim",
            vec![xbtc_type_tag.clone()],
            vec![faucet_arg.clone().into(), (10000 as u64).into()],
            &updated_gas_ref,
            gas_budget,
            reference_gas_price,
        );
        let effects = proxy.execute_transaction_block(transaction).await.unwrap();

        let created = effects.created();

        let mut xbtc_ref = None;
        for o in &created {
            let obj = proxy.get_object(o.0 .0).await.unwrap();
            if let Some(tag) = obj.data.struct_tag() {
                if tag.name.as_str().starts_with("Coin") {
                    xbtc_ref = Some(obj.compute_object_reference());
                    self.xbtc_coin_id = Some(o.0 .0);
                }
            }
        }

        info!("xbtc coin id: {:?}", self.xbtc_coin_id);

        let updated_gas_ref = proxy
            .get_object(gas.0 .0)
            .await
            .unwrap()
            .compute_object_reference();

        let transaction = move_call_pt_impl(
            gas.1,
            &gas.2,
            coin_package_obj.0 .0,
            "faucet",
            "force_claim",
            vec![usdt_type_tag.clone()],
            vec![faucet_arg.clone().into(), (10000 as u64).into()],
            &updated_gas_ref,
            gas_budget,
            reference_gas_price,
        );
        let effects = proxy.execute_transaction_block(transaction).await.unwrap();

        let created = effects.created();

        let mut usdt_ref = None;
        for o in &created {
            let obj = proxy.get_object(o.0 .0).await.unwrap();
            if let Some(tag) = obj.data.struct_tag() {
                if tag.name.as_str().starts_with("Coin") {
                    usdt_ref = Some(obj.compute_object_reference());
                    self.usdt_coin_id = Some(o.0 .0);
                }
            }
        }

        info!("usdt coin id: {:?}", self.usdt_coin_id);

        // 5. Add liquidity to the pool
        let updated_gas_ref = proxy
            .get_object(gas.0 .0)
            .await
            .unwrap()
            .compute_object_reference();

        let global_ref = global_ref.unwrap();
        let global_arg = CallArg::Object(ObjectArg::SharedObject {
            id: global_ref.0,
            initial_shared_version: global_ref.1,
            mutable: true,
        });
        self.global_arg = Some(global_arg.clone());
        let xbtc_ref = xbtc_ref.unwrap();
        let xbtc_arg = CallArg::Object(ObjectArg::ImmOrOwnedObject(xbtc_ref));
        let usdt_ref = usdt_ref.unwrap();
        let usdt_arg = CallArg::Object(ObjectArg::ImmOrOwnedObject(usdt_ref));

        let transaction = move_call_pt_impl(
            gas.1,
            &gas.2,
            samm_package_obj.0 .0,
            "interface",
            "add_liquidity",
            vec![usdt_type_tag.clone(), xbtc_type_tag.clone()],
            vec![
                global_arg.clone().into(),
                usdt_arg.clone().into(),
                (1 as u64).into(),
                xbtc_arg.clone().into(),
                (1 as u64).into(),
            ],
            &updated_gas_ref,
            gas_budget,
            reference_gas_price,
        );
        let effects = proxy.execute_transaction_block(transaction).await.unwrap();

        if effects.is_ok() {
            info!("liquidity added to XBTC-USDT pool");
        } else {
            error!("liquidity not added to XBTC-USDT pool {effects:?}");
        }

        // 6. Get large XBTC coin from faucet
        let updated_gas_ref = proxy
            .get_object(gas.0 .0)
            .await
            .unwrap()
            .compute_object_reference();

        let total_coin = 1000; // get total coin to be a relation of tps & num clients

        let _large_coin_amount = total_coin * (COIN_EACH_OBJ / ONECOIN);

        let transaction = move_call_pt_impl(
            gas.1,
            &gas.2,
            coin_package_obj.0 .0,
            "faucet",
            "force_claim",
            vec![xbtc_type_tag.clone()],
            vec![faucet_arg.clone().into(), (10000 as u64).into()],
            &updated_gas_ref,
            gas_budget,
            reference_gas_price,
        );
        let effects = proxy.execute_transaction_block(transaction).await.unwrap();

        let created = effects.created();

        let mut large_xbtc_ref = None;
        for o in &created {
            let obj = proxy.get_object(o.0 .0).await.unwrap();
            if let Some(tag) = obj.data.struct_tag() {
                if tag.name.as_str().starts_with("Coin") {
                    large_xbtc_ref = Some(obj.compute_object_reference());
                    self.large_xbtc_coin_id = Some(o.0 .0);
                }
            }
        }

        info!("large xbtc coin id: {:?}", self.large_xbtc_coin_id);

        // 7. Split coins into equal parts
        let updated_gas_ref = proxy
            .get_object(gas.0 .0)
            .await
            .unwrap()
            .compute_object_reference();

        let large_xbtc_ref = large_xbtc_ref.unwrap();
        let large_xbtc_arg = CallArg::Object(ObjectArg::ImmOrOwnedObject(large_xbtc_ref));

        let transaction = move_call_pt_impl(
            gas.1,
            &gas.2,
            SUI_FRAMEWORK_PACKAGE_ID,
            "pay",
            "divide_and_keep",
            vec![xbtc_type_tag.clone()],
            vec![large_xbtc_arg.clone().into(), (1000 as u64).into()],
            &updated_gas_ref,
            gas_budget,
            reference_gas_price,
        );

        let effects = proxy.execute_transaction_block(transaction).await.unwrap();
        let created = effects.created();

        for o in &created {
            let obj = proxy.get_object(o.0 .0).await.unwrap();
            if let Some(tag) = obj.data.struct_tag() {
                if tag.name.as_str().starts_with("Coin") {
                    self.xbtc_small_coin_refs
                        .push(obj.compute_object_reference());
                }
            }
        }

        info!(
            "xbtc small coin id count: {}",
            self.xbtc_small_coin_refs.len()
        );

        self.gas_budget = gas_budget;
    }

    async fn make_test_payloads(
        &self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        info!("make test payloads...");

        let mut samm_payloads = vec![];
        let global_arg = self.global_arg.clone().unwrap();
        for (i, g) in self.payload_gas.iter().enumerate() {
            let xbtc_small_coin_ref = self.xbtc_small_coin_refs[i];
            let xbtc_obj = proxy.get_object(xbtc_small_coin_ref.0).await.unwrap();
            // transfer xbtc to gas obj owner
            let transfer_from = xbtc_obj.owner().get_owner_address().unwrap();

            let gas = self
                .init_gas
                .first()
                .expect("Not enough gas to initialize samm workload");
            let gas_obj = proxy
                .get_object(gas.0 .0)
                .await
                .unwrap()
                .compute_object_reference();

            let transaction = make_transfer_object_transaction(
                xbtc_small_coin_ref.clone(),
                gas_obj,
                transfer_from,
                &gas.2,
                g.clone().1,
                system_state_observer.state.borrow().reference_gas_price,
            );
            let effects = proxy.execute_transaction_block(transaction).await.unwrap();

            if !effects.is_ok() {
                info!("failed {effects:?}");
            }

            let updated_gas_ref = proxy
                .get_object(g.0 .0)
                .await
                .unwrap()
                .compute_object_reference();
            let updated_gas = (updated_gas_ref, g.1, g.2.clone());

            let xbtc_small_coin_ref = proxy
                .get_object(xbtc_small_coin_ref.0)
                .await
                .unwrap()
                .compute_object_reference();

            samm_payloads.push(Box::new(SammTestPayload {
                samm_package_id: self.samm_package_id.unwrap(),
                coin_package_id: self.coins_package_id.unwrap(),
                global_arg: global_arg.clone(),
                xbtc_small_coin_ref,
                gas: updated_gas,
                gas_budget: self.gas_budget,
                system_state_observer: system_state_observer.clone(),
            }));
        }
        let payloads: Vec<Box<dyn Payload>> = samm_payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect();
        payloads
    }
}
