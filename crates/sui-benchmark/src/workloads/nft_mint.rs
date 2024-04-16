// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::util::publish_nfts_package;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{
    Workload, WorkloadBuilder, ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING,
    STORAGE_COST_PER_BYTE,
};
use crate::workloads::GasCoinConfig;
use crate::workloads::{Gas, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use fastcrypto::ed25519::Ed25519KeyPair;
use futures::future::join_all;
use memoize::memoize;
use rand::seq::SliceRandom;
use rand::Rng;
use rand::RngCore;
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::crypto::get_key_pair;
use sui_types::{
    base_types::{ObjectDigest, ObjectID, SequenceNumber, SuiAddress},
    transaction::Transaction,
};
use tracing::{debug, error, info};

/// The max amount of gas units needed for a payload.
pub const MAX_GAS_IN_UNIT: u64 = 1_000_000_000;

#[derive(Debug)]
pub struct NFTMintTestPayload {
    package_id: ObjectID,
    recipient: SuiAddress,
    nft_contents_size: u64,
    gas: Gas,
    system_state_observer: Arc<SystemStateObserver>,
}

impl std::fmt::Display for NFTMintTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "mint payload")
    }
}

impl Payload for NFTMintTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        if !effects.is_ok() {
            effects.print_gas_summary();
            error!("Mint tx failed...");
        }
        // else {
        //     error!("{:#?}", effects.status());
        //     error!("{:#?}", effects);
        // }
        self.gas.0 = effects.gas_object().0;
    }

    fn make_transaction(&mut self) -> Transaction {
        let rgp = self
            .system_state_observer
            .state
            .borrow()
            .reference_gas_price;

        TestTransactionBuilder::new(self.gas.1, self.gas.0, rgp)
            .call_nft_mint_one(
                self.package_id,
                generate_nft_contents(self.nft_contents_size),
                self.recipient,
            )
            .build_and_sign(self.gas.2.as_ref())
    }
}

#[derive(Debug)]
pub struct NFTMintWorkloadBuilder {
    num_payloads: u64,
    nft_contents_size: u64,
    rgp: u64,
}

impl NFTMintWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        nft_contents_size: u64,
        reference_gas_price: u64,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32) as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        let workload_params = WorkloadParams {
            group,
            target_qps,
            num_workers,
            max_ops,
            duration,
        };
        let workload_builder =
            Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(NFTMintWorkloadBuilder {
                num_payloads: max_ops,
                nft_contents_size,
                rgp: reference_gas_price,
            }));
        let builder_info = WorkloadBuilderInfo {
            workload_params,
            workload_builder,
        };
        Some(builder_info)
    }
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for NFTMintWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        let mut configs = vec![];

        // Gas coin for publishing package
        let (address, keypair) = get_key_pair();
        configs.push(GasCoinConfig {
            amount: MAX_GAS_FOR_TESTING,
            address,
            keypair: Arc::new(keypair),
        });
        configs
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut configs = vec![];
        let amount = MAX_GAS_IN_UNIT * self.rgp
            + ESTIMATED_COMPUTATION_COST
            + STORAGE_COST_PER_BYTE * self.nft_contents_size;
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
        let (recipient, _keypair) = get_key_pair::<Ed25519KeyPair>();
        Box::<dyn Workload<dyn Payload>>::from(Box::new(NFTMintWorkload {
            basics_package_id: None,
            init_gas,
            payload_gas,
            nft_contents_size: self.nft_contents_size,
            recipient,
        }))
    }
}

#[derive(Debug)]
pub struct NFTMintWorkload {
    pub basics_package_id: Option<ObjectID>,
    pub init_gas: Vec<Gas>,
    pub payload_gas: Vec<Gas>,
    pub nft_contents_size: u64,
    pub recipient: SuiAddress,
}

#[async_trait]
impl Workload<dyn Payload> for NFTMintWorkload {
    async fn init(
        &mut self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        if self.basics_package_id.is_some() {
            return;
        }
        let gas_price = system_state_observer.state.borrow().reference_gas_price;
        let (head, tail) = self
            .init_gas
            .split_first()
            .expect("Not enough gas to initialize mint workload");

        // Publish basics package
        info!("Publishing basics package");
        self.basics_package_id = Some(
            publish_nfts_package(head.0, proxy.clone(), head.1, &head.2, gas_price)
                .await
                .0,
        );
        info!("Basics package id {:?}", self.basics_package_id);
    }

    async fn make_test_payloads(
        &self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        info!("Creating mint one txn payloads, hang tight..");
        let mut mint_payloads = vec![];
        for g in self.payload_gas.iter() {
            mint_payloads.push(Box::new(NFTMintTestPayload {
                package_id: self.basics_package_id.unwrap(),
                gas: g.clone(),
                system_state_observer: system_state_observer.clone(),
                nft_contents_size: self.nft_contents_size,
                recipient: self.recipient,
            }));
        }
        let payloads: Vec<Box<dyn Payload>> = mint_payloads
            .into_iter()
            .map(|b| Box::<dyn Payload>::from(b))
            .collect();
        payloads
    }
}

#[memoize]
fn generate_nft_contents(size: u64) -> Vec<u8> {
    let mut nft_contents = vec![0u8; size as usize];
    rand::thread_rng().fill_bytes(&mut nft_contents);

    nft_contents
}
