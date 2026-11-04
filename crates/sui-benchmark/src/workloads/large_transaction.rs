// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Benchmark workload that generates transactions of a configurable size, for measuring
//! consensus throughput as a function of block size.
//!
//! Gas is paid from a gas coin by default. With address balance gas, a transaction that the
//! client gave up on cannot make the sender's later transactions stale.

use super::{
    WorkloadBuilderInfo, WorkloadParams,
    workload::{MAX_GAS_FOR_TESTING, Workload, WorkloadBuilder},
};
use crate::drivers::Interval;
use crate::in_memory_wallet::InMemoryWallet;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::{Gas, GasCoinConfig, workload::ExpectedFailureType};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use futures::StreamExt;
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{AccountKeyPair, get_key_pair};
use sui_types::digests::ChainIdentifier;
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Argument, Command, Transaction, TransactionData};
use sui_types::type_input::TypeInput;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID};
use tracing::info;

/// Leave room for the transaction envelope (sender, gas, signature, command) under
/// `max_tx_size_bytes`. Measured envelope is well under 1 KB; 8 KB is a safe margin.
const ENVELOPE_HEADROOM_BYTES: u64 = 8 * 1024;

/// Gas units budgeted per transaction, with generous headroom over the expected cost.
const GAS_UNITS_PER_TX: u64 = 1_000_000;

/// Gas budget for the transaction that moves a payload's funds into its address balance.
const SEED_GAS_BUDGET: u64 = 50_000_000;

/// Concurrent address balance seeding transactions per stress client.
const SEED_CONCURRENCY: usize = 64;

#[derive(Debug, Clone, Copy)]
pub struct LargeTransactionSizeRange {
    pub min_bytes: u64,
    pub max_bytes: u64,
}

impl LargeTransactionSizeRange {
    /// A zero minimum means every transaction carries `max_bytes`.
    pub fn new(min_bytes: u64, max_bytes: u64) -> anyhow::Result<Self> {
        let min_bytes = if min_bytes == 0 { max_bytes } else { min_bytes };
        anyhow::ensure!(
            min_bytes <= max_bytes,
            "large transaction minimum size {min_bytes} exceeds maximum size {max_bytes}"
        );
        Ok(Self {
            min_bytes,
            max_bytes,
        })
    }

    fn sample(&self, rng: &mut SmallRng) -> u64 {
        rng.gen_range(self.min_bytes..=self.max_bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeTransactionGas {
    Coin,
    AddressBalance,
}

#[derive(Debug)]
enum PayloadGas {
    Coin(InMemoryWallet),
    AddressBalance {
        keypair: Arc<AccountKeyPair>,
        chain_identifier: ChainIdentifier,
        nonce: u32,
    },
}

#[derive(Debug)]
pub struct LargeTransactionTestPayload {
    size: LargeTransactionSizeRange,
    sender: SuiAddress,
    gas: PayloadGas,
    system_state_observer: Arc<SystemStateObserver>,
    rng: SmallRng,
}

impl std::fmt::Display for LargeTransactionTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "large_transaction")
    }
}

impl Payload for LargeTransactionTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        debug_assert!(
            effects.is_ok() || effects.is_cancelled(),
            "Large transactions should never abort: {effects:?}",
        );
        if let PayloadGas::Coin(state) = &mut self.gas {
            state.update(effects);
        }
    }

    fn make_transaction(&mut self) -> Transaction {
        self.create_transaction()
    }

    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None
    }
}

impl LargeTransactionTestPayload {
    fn create_transaction(&mut self) -> Transaction {
        let (gas_price, max_pure_arg_size, max_tx_size, epoch) = {
            let state = self.system_state_observer.state.borrow();
            let cfg = state
                .protocol_config
                .as_ref()
                .expect("Protocol config not in system state");
            (
                state.reference_gas_price,
                cfg.max_pure_argument_size() as u64,
                cfg.max_tx_size_bytes(),
                state.epoch,
            )
        };

        // Stay under the per-input size limit, accounting for the BCS length prefix.
        let per_input = max_pure_arg_size.saturating_sub(16);
        let budget = max_tx_size.saturating_sub(ENVELOPE_HEADROOM_BYTES);
        let total = self.size.sample(&mut self.rng).min(budget);

        let mut builder = ProgrammableTransactionBuilder::new();

        builder.command(Command::MakeMoveVec(Some(TypeInput::U8), vec![]));

        // Attach the payload as separate pure inputs.
        let mut remaining = total;
        while remaining > 0 {
            let n = remaining.min(per_input);
            let mut chunk = vec![0u8; n as usize];
            self.rng.fill(&mut chunk[..]);
            builder.pure_bytes(chunk, /* force_separate */ true);
            remaining -= n;
        }

        let gas_budget = gas_price * GAS_UNITS_PER_TX;
        let tx = match &mut self.gas {
            PayloadGas::Coin(state) => {
                let account = state.account(&self.sender).unwrap();
                let data = TransactionData::new_programmable(
                    self.sender,
                    vec![account.gas],
                    builder.finish(),
                    gas_budget,
                    gas_price,
                );
                to_sender_signed_transaction(data, account.key())
            }
            PayloadGas::AddressBalance {
                keypair,
                chain_identifier,
                nonce,
            } => {
                let data = TransactionData::new_programmable_with_address_balance_gas(
                    self.sender,
                    builder.finish(),
                    gas_budget,
                    gas_price,
                    *chain_identifier,
                    epoch,
                    *nonce,
                );
                *nonce = nonce.wrapping_add(1);
                to_sender_signed_transaction(data, keypair.as_ref())
            }
        };
        // Assert the full payload made it into the transaction.
        debug_assert!(
            bcs::to_bytes(&tx).map(|b| b.len() as u64).unwrap_or(0) >= total,
            "payload did not reach the transaction: requested {total} bytes",
        );
        tx
    }
}

#[derive(Debug)]
pub struct LargeTransactionWorkloadBuilder {
    num_payloads: u64,
    size: LargeTransactionSizeRange,
    gas: LargeTransactionGas,
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for LargeTransactionWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        // No package to publish.
        vec![]
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        (0..self.num_payloads)
            .map(|_| {
                let (address, keypair) = get_key_pair();
                GasCoinConfig {
                    amount: MAX_GAS_FOR_TESTING,
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
        Box::<dyn Workload<dyn Payload>>::from(Box::new(LargeTransactionWorkload {
            size: self.size,
            gas: self.gas,
            payload_gas,
            chain_identifier: None,
        }))
    }
}

impl LargeTransactionWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        size: LargeTransactionSizeRange,
        gas: LargeTransactionGas,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32).ceil() as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            return None;
        }
        let workload_params = WorkloadParams {
            target_qps,
            num_workers,
            max_ops,
            duration,
            group,
        };
        let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
            LargeTransactionWorkloadBuilder {
                num_payloads: max_ops,
                size,
                gas,
            },
        ));
        Some(WorkloadBuilderInfo {
            workload_params,
            workload_builder,
        })
    }
}

#[derive(Debug)]
pub struct LargeTransactionWorkload {
    size: LargeTransactionSizeRange,
    gas: LargeTransactionGas,
    pub payload_gas: Vec<Gas>,
    chain_identifier: Option<ChainIdentifier>,
}

/// Moves all but the seeding transaction's own gas from the payload's gas coin into the
/// address balance that later pays for the large transactions.
fn seed_address_balance_transaction(gas: &Gas, gas_price: u64) -> Transaction {
    let (coin, sender, keypair) = gas;
    let seed_amount = MAX_GAS_FOR_TESTING - 2 * SEED_GAS_BUDGET;
    let mut tx_builder =
        TestTransactionBuilder::new(*sender, *coin, gas_price).with_gas_budget(SEED_GAS_BUDGET);
    {
        let builder = tx_builder.ptb_builder_mut();
        let amount_arg = builder.pure(seed_amount).unwrap();
        let Argument::Result(coin_idx) =
            builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]))
        else {
            panic!("SplitCoins should return Result");
        };
        let recipient_arg = builder.pure(*sender).unwrap();
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("send_funds").unwrap(),
            vec![GAS::type_tag()],
            vec![Argument::NestedResult(coin_idx, 0), recipient_arg],
        );
    }
    tx_builder.build_and_sign(keypair.as_ref())
}

#[async_trait]
impl Workload<dyn Payload> for LargeTransactionWorkload {
    async fn init(
        &mut self,
        execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        system_state_observer: Arc<SystemStateObserver>,
    ) {
        if self.gas == LargeTransactionGas::Coin {
            return;
        }
        self.chain_identifier = Some(execution_proxy.get_chain_identifier());
        let gas_price = system_state_observer.state.borrow().reference_gas_price;
        info!(
            "Seeding address balances for {} large transaction senders",
            self.payload_gas.len()
        );
        let seed_txs: Vec<Transaction> = self
            .payload_gas
            .iter()
            .map(|gas| seed_address_balance_transaction(gas, gas_price))
            .collect();
        futures::stream::iter(seed_txs)
            .map(|tx| {
                let proxy = execution_proxy.clone();
                async move {
                    let effects = proxy
                        .execute_transaction_block(tx)
                        .await
                        .expect("address balance seeding should succeed");
                    assert!(
                        effects.is_ok(),
                        "address balance seeding aborted: {:?}",
                        effects.status()
                    );
                }
            })
            .buffer_unordered(SEED_CONCURRENCY)
            .collect::<Vec<()>>()
            .await;
        info!("Seeded {} address balances", self.payload_gas.len());
    }

    async fn make_test_payloads(
        &self,
        _execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        self.payload_gas
            .iter()
            .enumerate()
            .map(|(i, gas)| {
                let payload_gas = match self.gas {
                    LargeTransactionGas::Coin => PayloadGas::Coin(InMemoryWallet::new(gas)),
                    LargeTransactionGas::AddressBalance => PayloadGas::AddressBalance {
                        keypair: gas.2.clone(),
                        chain_identifier: self
                            .chain_identifier
                            .expect("init must run before make_test_payloads"),
                        nonce: 0,
                    },
                };
                Box::<dyn Payload>::from(Box::new(LargeTransactionTestPayload {
                    size: self.size,
                    sender: gas.1,
                    gas: payload_gas,
                    system_state_observer: system_state_observer.clone(),
                    rng: SmallRng::seed_from_u64(i as u64),
                }))
            })
            .collect()
    }

    fn name(&self) -> &str {
        "LargeTransaction"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_range_zero_min_means_fixed_size() {
        let range = LargeTransactionSizeRange::new(0, 100_000).unwrap();
        assert_eq!((range.min_bytes, range.max_bytes), (100_000, 100_000));
        let mut rng = SmallRng::seed_from_u64(1);
        assert!((0..100).all(|_| range.sample(&mut rng) == 100_000));
    }

    #[test]
    fn size_range_samples_within_bounds() {
        let range = LargeTransactionSizeRange::new(1_000, 4_000).unwrap();
        let mut rng = SmallRng::seed_from_u64(7);
        let samples: Vec<u64> = (0..1_000).map(|_| range.sample(&mut rng)).collect();
        assert!(samples.iter().all(|s| (1_000..=4_000).contains(s)));
        assert!(samples.iter().any(|s| *s < 2_000));
        assert!(samples.iter().any(|s| *s > 3_000));
    }

    #[test]
    fn size_range_rejects_min_above_max() {
        assert!(LargeTransactionSizeRange::new(5_000, 4_000).is_err());
    }
}
