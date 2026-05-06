// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::{BatchExecutionResults, BatchedTransactionStatus, Payload};
use crate::workloads::workload::{ESTIMATED_COMPUTATION_COST, Workload, WorkloadBuilder};
use crate::workloads::{Gas, GasCoinConfig, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use futures::future::join_all;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{AccountKeyPair, get_key_pair};
use sui_types::digests::ChainIdentifier;
use sui_types::gas_coin::GAS;
use sui_types::transaction::{Argument, Command, FundsWithdrawalArg, Transaction};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID};
use tracing::info;

const GAS_BUDGET: u64 = 50_000_000;

#[derive(Clone)]
pub struct AddrBalDepositConfig {
    pub target_addresses: Vec<SuiAddress>,
    pub deposit_amount: u64,
    pub seed_amount: u64,
    pub metrics: Option<Arc<Mutex<AddrBalDepositMetrics>>>,
}

impl std::fmt::Debug for AddrBalDepositConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddrBalDepositConfig")
            .field("target_addresses", &self.target_addresses)
            .field("deposit_amount", &self.deposit_amount)
            .field("seed_amount", &self.seed_amount)
            .finish()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AddrBalDepositMetrics {
    pub sent: u64,
    pub success: u64,
    pub abort: u64,
    pub permanent_failure: u64,
    pub retriable_failure: u64,
    pub unknown_rejection: u64,
}

pub struct AddrBalDepositWorkloadBuilder {
    config: AddrBalDepositConfig,
    num_payloads: u64,
    num_workers: u64,
}

impl std::fmt::Debug for AddrBalDepositWorkloadBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddrBalDepositWorkloadBuilder")
            .field("config", &self.config)
            .field("num_payloads", &self.num_payloads)
            .finish()
    }
}

impl AddrBalDepositWorkloadBuilder {
    pub fn build_info(
        config: AddrBalDepositConfig,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            return None;
        }

        let workload_params = WorkloadParams {
            group,
            target_qps,
            num_workers,
            max_ops,
            duration,
        };
        let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
            AddrBalDepositWorkloadBuilder {
                config,
                num_payloads: max_ops,
                num_workers,
            },
        ));

        Some(WorkloadBuilderInfo {
            workload_params,
            workload_builder,
        })
    }
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for AddrBalDepositWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        vec![]
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let amount = self.config.seed_amount + GAS_BUDGET + ESTIMATED_COMPUTATION_COST;
        (0..self.num_workers)
            .map(|_| {
                let (address, keypair) = get_key_pair::<AccountKeyPair>();
                GasCoinConfig {
                    amount,
                    address,
                    keypair: Arc::new(keypair),
                }
            })
            .collect()
    }

    async fn build(
        &self,
        _init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(AddrBalDepositWorkload {
            config: self.config.clone(),
            payload_gas,
            num_payloads: self.num_payloads,
            chain_identifier: None,
            metrics: self
                .config
                .metrics
                .clone()
                .unwrap_or_else(|| Arc::new(Mutex::new(AddrBalDepositMetrics::default()))),
        }))
    }
}

#[derive(Debug)]
pub struct AddrBalDepositWorkload {
    config: AddrBalDepositConfig,
    payload_gas: Vec<Gas>,
    num_payloads: u64,
    chain_identifier: Option<ChainIdentifier>,
    metrics: Arc<Mutex<AddrBalDepositMetrics>>,
}

impl AddrBalDepositWorkload {
    pub fn metrics(&self) -> Arc<Mutex<AddrBalDepositMetrics>> {
        self.metrics.clone()
    }
}

#[async_trait]
impl Workload<dyn Payload> for AddrBalDepositWorkload {
    async fn init(
        &mut self,
        execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        self.chain_identifier = Some(execution_proxy.get_chain_identifier());
        let gas_price = system_state_observer.state.borrow().reference_gas_price;

        info!(
            "Seeding address balances with {} MIST for {} senders",
            self.config.seed_amount,
            self.payload_gas.len(),
        );

        let mut futures = vec![];
        for (idx, (gas, sender, keypair)) in self.payload_gas.iter().enumerate() {
            let gas = *gas;
            let seed_amount = self.config.seed_amount;

            let mut tx_builder =
                TestTransactionBuilder::new(*sender, gas, gas_price).with_gas_budget(GAS_BUDGET);
            {
                let builder = tx_builder.ptb_builder_mut();
                let amount_arg = builder.pure(seed_amount).unwrap();
                let coin =
                    builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));
                let Argument::Result(coin_idx) = coin else {
                    panic!("SplitCoins should return Result");
                };
                let coin = Argument::NestedResult(coin_idx, 0);
                let coin_balance = builder.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("coin").unwrap(),
                    Identifier::new("into_balance").unwrap(),
                    vec![GAS::type_tag()],
                    vec![coin],
                );
                let recipient_arg = builder.pure(*sender).unwrap();
                builder.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("balance").unwrap(),
                    Identifier::new("send_funds").unwrap(),
                    vec![GAS::type_tag()],
                    vec![coin_balance, recipient_arg],
                );
            }
            let tx = tx_builder.build_and_sign(keypair.as_ref());
            let proxy_ref = execution_proxy.clone();
            futures.push(async move {
                let result = proxy_ref.execute_transaction_block(tx).await;
                let effects = result.expect("Seed deposit should succeed");
                (idx, effects)
            });
        }

        let results = join_all(futures).await;
        for (idx, effects) in results {
            let (new_gas_ref, _) = effects.gas_object();
            self.payload_gas[idx].0 = new_gas_ref;
        }
        info!("Seeded {} address balances", self.payload_gas.len());
    }

    async fn make_test_payloads(
        &self,
        _execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let chain_id = self.chain_identifier.unwrap();
        let num_senders = self.payload_gas.len();

        // Create one shared nonce counter per sender so payloads sharing
        // a sender produce unique address-balance-gas nonces.
        let nonce_counters: Vec<Arc<AtomicU32>> = (0..num_senders)
            .map(|_| Arc::new(AtomicU32::new(0)))
            .collect();

        let mut payloads: Vec<Box<dyn Payload>> = vec![];
        for i in 0..self.num_payloads {
            let sender_idx = i as usize % num_senders;
            let (_, sender, ref keypair) = self.payload_gas[sender_idx];
            payloads.push(Box::new(AddrBalDepositPayload {
                target_addresses: self.config.target_addresses.clone(),
                deposit_amount: self.config.deposit_amount,
                sender,
                keypair: keypair.clone(),
                chain_identifier: chain_id,
                system_state_observer: system_state_observer.clone(),
                nonce_counter: nonce_counters[sender_idx].clone(),
                metrics: self.metrics.clone(),
            }));
        }

        info!(
            "Created {} addr_bal_deposit payloads across {} senders",
            payloads.len(),
            num_senders,
        );
        payloads
    }

    fn name(&self) -> &str {
        "addr_bal_deposit"
    }
}

pub struct AddrBalDepositPayload {
    target_addresses: Vec<SuiAddress>,
    deposit_amount: u64,
    sender: SuiAddress,
    keypair: Arc<AccountKeyPair>,
    chain_identifier: ChainIdentifier,
    system_state_observer: Arc<SystemStateObserver>,
    nonce_counter: Arc<AtomicU32>,
    metrics: Arc<Mutex<AddrBalDepositMetrics>>,
}

impl std::fmt::Debug for AddrBalDepositPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddrBalDepositPayload")
            .field("targets", &self.target_addresses.len())
            .finish()
    }
}

impl std::fmt::Display for AddrBalDepositPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "addr_bal_deposit")
    }
}

#[async_trait]
impl Payload for AddrBalDepositPayload {
    fn make_new_payload(&mut self, _: &ExecutionEffects) {
        unreachable!();
    }

    fn make_transaction(&mut self) -> Transaction {
        unreachable!()
    }

    fn is_batched(&self) -> bool {
        true
    }

    async fn make_transaction_batch(&mut self) -> Vec<Transaction> {
        let system_state = self.system_state_observer.state.borrow().clone();
        let rgp = system_state.reference_gas_price;
        let current_epoch = system_state.epoch;

        let nonce = self.nonce_counter.fetch_add(1, Ordering::Relaxed);
        let mut tx_builder = TestTransactionBuilder::new_with_address_balance_gas(
            self.sender,
            rgp,
            self.chain_identifier,
            current_epoch,
            nonce,
        )
        .with_gas_budget(GAS_BUDGET);

        {
            let builder = tx_builder.ptb_builder_mut();

            for target in &self.target_addresses {
                let withdrawal =
                    FundsWithdrawalArg::balance_from_sender(self.deposit_amount, GAS::type_tag());
                let withdrawal_result = builder.funds_withdrawal(withdrawal).unwrap();

                let balance = builder.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("balance").unwrap(),
                    Identifier::new("redeem_funds").unwrap(),
                    vec![GAS::type_tag()],
                    vec![withdrawal_result],
                );

                let recipient_arg = builder.pure(*target).unwrap();
                builder.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("balance").unwrap(),
                    Identifier::new("send_funds").unwrap(),
                    vec![GAS::type_tag()],
                    vec![balance, recipient_arg],
                );
            }
        }

        let tx = tx_builder.build_and_sign(self.keypair.as_ref());
        self.metrics.lock().unwrap().sent += 1;
        vec![tx]
    }

    fn handle_batch_results(&mut self, results: &BatchExecutionResults) {
        let mut metrics = self.metrics.lock().unwrap();
        for result in &results.results {
            match &result.status {
                BatchedTransactionStatus::Success { effects } => {
                    if effects.is_ok() {
                        metrics.success += 1;
                        if metrics.success % 100 == 1 {
                            tracing::warn!(
                                "addr_bal_deposit: {} successful deposits to {} target(s) so far",
                                metrics.success,
                                self.target_addresses.len(),
                            );
                        }
                    } else {
                        tracing::warn!(
                            "addr_bal_deposit tx {} aborted: {:?}",
                            result.digest,
                            effects.status(),
                        );
                        metrics.abort += 1;
                    }
                }
                BatchedTransactionStatus::PermanentFailure { error } => {
                    tracing::warn!(
                        "addr_bal_deposit tx {} permanent failure: {}",
                        result.digest,
                        error,
                    );
                    metrics.permanent_failure += 1;
                }
                BatchedTransactionStatus::RetriableFailure { error } => {
                    tracing::warn!(
                        "addr_bal_deposit tx {} retriable failure: {}",
                        result.digest,
                        error,
                    );
                    metrics.retriable_failure += 1;
                }
                BatchedTransactionStatus::UnknownRejection => {
                    metrics.unknown_rejection += 1;
                }
            }
        }
    }
}
