// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use fullnode_reconfig_observer::FullNodeReconfigObserver;
use futures::TryStreamExt;
use mysten_common::{fatal, random::get_rng};
use rand::{Rng, seq::IteratorRandom};
use sui_config::genesis::Genesis;
use sui_core::{
    authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder},
    authority_client::NetworkAuthorityClient,
    epoch::committee_store::CommitteeStore,
    safe_client::SafeClientMetricsBase,
    transaction_driver::{
        ReconfigObserver, SubmitTransactionOptions, TransactionDriver, TransactionDriverMetrics,
    },
    validator_client_monitor::ValidatorClientMetrics,
};
use sui_protocol_config::ProtocolConfig;
use sui_rpc_api::{Client, client::ExecutedTransaction};
use sui_types::transaction::Argument;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction_driver_types::EffectsFinalityInfo;
use sui_types::transaction_driver_types::FinalizedEffects;
use sui_types::{
    base_types::ObjectID,
    committee::{Committee, EpochId},
    object::Object,
    transaction::Transaction,
};
use sui_types::{base_types::ObjectRef, crypto::AuthorityStrongQuorumSignInfo, object::Owner};
use sui_types::{base_types::SequenceNumber, gas_coin::GasCoin};
use sui_types::{
    base_types::TransactionDigest,
    messages_grpc::{
        RawSubmitTxRequest, SubmitTxRequest, SubmitTxResult, SubmitTxType, WaitForEffectsRequest,
        WaitForEffectsResponse,
    },
    programmable_transaction_builder::ProgrammableTransactionBuilder,
};
use sui_types::{
    base_types::{AuthorityName, SuiAddress},
    sui_system_state::SuiSystemStateTrait,
};
use sui_types::{
    digests::ChainIdentifier, gas::GasCostSummary, transaction::SharedObjectMutability,
};
use sui_types::{
    effects::{TransactionEffectsAPI, TransactionEvents},
    execution_status::ExecutionFailureStatus,
};
use sui_types::{gas_coin::GAS, sui_system_state::sui_system_state_summary::SuiSystemStateSummary};
use tokio::time::sleep;
use tracing::{debug, info, instrument, warn};

use crate::drivers::bench_driver::ClientType;

pub mod bank;

/// Shared metrics for benchmark proxies that use TransactionDriver.
/// Creating these metrics multiple times with the same registry would cause
/// duplicate metric registration panics, so they must be shared.
#[derive(Clone)]
pub struct BenchmarkProxyMetrics {
    pub safe_client_metrics_base: SafeClientMetricsBase,
    pub transaction_driver_metrics: Arc<TransactionDriverMetrics>,
    pub client_metrics: Arc<ValidatorClientMetrics>,
}

impl BenchmarkProxyMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
        Self {
            safe_client_metrics_base: SafeClientMetricsBase::new(registry),
            transaction_driver_metrics: Arc::new(TransactionDriverMetrics::new(registry)),
            client_metrics: Arc::new(ValidatorClientMetrics::new(registry)),
        }
    }
}
pub mod benchmark_setup;
pub mod drivers;
pub mod fullnode_reconfig_observer;
pub mod in_memory_wallet;
pub mod options;
pub mod system_state_observer;
pub mod util;
pub mod workloads;

#[derive(Debug)]
/// A wrapper on execution results to accommodate different types of
/// responses from LocalValidatorAggregatorProxy and FullNodeProxy
#[allow(clippy::large_enum_variant)]
pub enum ExecutionEffects {
    FinalizedTransactionEffects(FinalizedEffects, TransactionEvents),
    ExecutedTransaction(ExecutedTransaction),
}

impl ExecutionEffects {
    pub fn digest(&self) -> TransactionDigest {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                *effects.data().transaction_digest()
            }
            ExecutionEffects::ExecutedTransaction(txn) => *txn.effects.transaction_digest(),
        }
    }

    pub fn mutated(&self) -> Vec<(ObjectRef, Owner)> {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                effects.data().mutated().to_vec()
            }
            ExecutionEffects::ExecutedTransaction(txn) => txn.effects.mutated(),
        }
    }

    pub fn created(&self) -> Vec<(ObjectRef, Owner)> {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => effects.data().created(),
            ExecutionEffects::ExecutedTransaction(txn) => txn.effects.created(),
        }
    }

    pub fn deleted(&self) -> Vec<ObjectRef> {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                effects.data().deleted().to_vec()
            }
            ExecutionEffects::ExecutedTransaction(txn) => txn.effects.deleted(),
        }
    }

    pub fn quorum_sig(&self) -> Option<&AuthorityStrongQuorumSignInfo> {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                match &effects.finality_info {
                    EffectsFinalityInfo::Certified(sig) => Some(sig),
                    _ => None,
                }
            }
            ExecutionEffects::ExecutedTransaction(_) => None,
        }
    }

    pub fn gas_object(&self) -> (ObjectRef, Owner) {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                effects.data().gas_object()
            }
            ExecutionEffects::ExecutedTransaction(txn) => txn.effects.gas_object(),
        }
    }

    pub fn sender(&self) -> SuiAddress {
        match self.gas_object().1 {
            Owner::AddressOwner(a) => a,
            Owner::ObjectOwner(_)
            | Owner::Shared { .. }
            | Owner::Immutable
            | Owner::ConsensusAddressOwner { .. } => unreachable!(), // owner of gas object is always an address
        }
    }

    pub fn is_ok(&self) -> bool {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                effects.data().status().is_ok()
            }
            ExecutionEffects::ExecutedTransaction(txn) => txn.effects.status().is_ok(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                match effects.data().status() {
                    sui_types::execution_status::ExecutionStatus::Success => false,
                    sui_types::execution_status::ExecutionStatus::Failure {
                        error:
                            ExecutionFailureStatus::ExecutionCancelledDueToSharedObjectCongestion {
                                ..
                            },
                        ..
                    } => true,
                    _ => false,
                }
            }
            ExecutionEffects::ExecutedTransaction(txn) => match txn.effects.status() {
                sui_types::execution_status::ExecutionStatus::Success => false,
                sui_types::execution_status::ExecutionStatus::Failure {
                    error:
                        ExecutionFailureStatus::ExecutionCancelledDueToSharedObjectCongestion { .. },
                    ..
                } => true,
                _ => false,
            },
        }
    }

    pub fn is_insufficient_funds(&self) -> bool {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                match effects.data().status() {
                    sui_types::execution_status::ExecutionStatus::Success => false,
                    sui_types::execution_status::ExecutionStatus::Failure {
                        error: ExecutionFailureStatus::InsufficientFundsForWithdraw,
                        ..
                    } => true,
                    _ => false,
                }
            }
            ExecutionEffects::ExecutedTransaction(txn) => match txn.effects.status() {
                sui_types::execution_status::ExecutionStatus::Success => false,
                sui_types::execution_status::ExecutionStatus::Failure {
                    error: ExecutionFailureStatus::InsufficientFundsForWithdraw,
                    ..
                } => true,
                _ => false,
            },
        }
    }

    pub fn is_invalid_transaction(&self) -> bool {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                match effects.data().status() {
                    sui_types::execution_status::ExecutionStatus::Failure { error, .. } => {
                        matches!(
                            error,
                            ExecutionFailureStatus::VMVerificationOrDeserializationError
                                | ExecutionFailureStatus::VMInvariantViolation
                                | ExecutionFailureStatus::FunctionNotFound
                                | ExecutionFailureStatus::ArityMismatch
                                | ExecutionFailureStatus::TypeArityMismatch
                                | ExecutionFailureStatus::NonEntryFunctionInvoked
                                | ExecutionFailureStatus::CommandArgumentError { .. }
                                | ExecutionFailureStatus::TypeArgumentError { .. }
                                | ExecutionFailureStatus::UnusedValueWithoutDrop { .. }
                                | ExecutionFailureStatus::InvalidPublicFunctionReturnType { .. }
                                | ExecutionFailureStatus::InvalidTransferObject
                        )
                    }
                    _ => false,
                }
            }
            ExecutionEffects::ExecutedTransaction(txn) => match txn.effects.status() {
                sui_types::execution_status::ExecutionStatus::Failure { error, .. } => {
                    matches!(
                        error,
                        ExecutionFailureStatus::VMVerificationOrDeserializationError
                            | ExecutionFailureStatus::VMInvariantViolation
                            | ExecutionFailureStatus::FunctionNotFound
                            | ExecutionFailureStatus::ArityMismatch
                            | ExecutionFailureStatus::TypeArityMismatch
                            | ExecutionFailureStatus::NonEntryFunctionInvoked
                            | ExecutionFailureStatus::CommandArgumentError { .. }
                            | ExecutionFailureStatus::TypeArgumentError { .. }
                            | ExecutionFailureStatus::UnusedValueWithoutDrop { .. }
                            | ExecutionFailureStatus::InvalidPublicFunctionReturnType { .. }
                            | ExecutionFailureStatus::InvalidTransferObject
                    )
                }
                _ => false,
            },
        }
    }

    pub fn status(&self) -> String {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                format!("{:#?}", effects.data().status())
            }
            ExecutionEffects::ExecutedTransaction(txn) => {
                format!("{:#?}", txn.effects.status())
            }
        }
    }

    pub fn gas_cost_summary(&self) -> GasCostSummary {
        match self {
            crate::ExecutionEffects::FinalizedTransactionEffects(a, _) => {
                a.data().gas_cost_summary().clone()
            }
            ExecutionEffects::ExecutedTransaction(txn) => txn.effects.gas_cost_summary().clone(),
        }
    }

    pub fn gas_used(&self) -> u64 {
        self.gas_cost_summary().gas_used()
    }

    pub fn net_gas_used(&self) -> i64 {
        self.gas_cost_summary().net_gas_usage()
    }

    pub fn print_gas_summary(&self) {
        let gas_object = self.gas_object();
        let sender = self.sender();
        let status = self.status();
        let gas_cost_summary = self.gas_cost_summary();
        let gas_used = self.gas_used();
        let net_gas_used = self.net_gas_used();

        info!(
            "Summary:\n\
             Gas Object: {gas_object:?}\n\
             Sender: {sender:?}\n\
             status: {status}\n\
             Gas Cost Summary: {gas_cost_summary:#?}\n\
             Gas Used: {gas_used}\n\
             Net Gas Used: {net_gas_used}"
        );
    }
}

#[async_trait]
pub trait ValidatorProxy {
    async fn get_object(&self, object_id: ObjectID) -> Result<Object, anyhow::Error>;

    async fn get_sui_address_balance(&self, address: SuiAddress) -> Result<u64, anyhow::Error>;

    async fn get_owned_objects(
        &self,
        account_address: SuiAddress,
    ) -> Result<Vec<(u64, Object)>, anyhow::Error>;

    async fn get_latest_system_state_object(&self) -> Result<SuiSystemStateSummary, anyhow::Error>;

    async fn execute_transaction_block(
        &self,
        tx: Transaction,
    ) -> (ClientType, anyhow::Result<ExecutionEffects>);

    fn clone_committee(&self) -> Arc<Committee>;

    fn get_current_epoch(&self) -> EpochId;

    fn clone_new(&self) -> Box<dyn ValidatorProxy + Send + Sync>;

    async fn get_validators(&self) -> Result<Vec<SuiAddress>, anyhow::Error>;

    /// Execute multiple transactions as a soft bundle.
    /// Soft bundles guarantee that all transactions are ordered together in consensus,
    /// preserving their relative order within the bundle.
    /// Returns a vector of (digest, response) for each transaction.
    async fn execute_soft_bundle(
        &self,
        txs: Vec<Transaction>,
    ) -> anyhow::Result<Vec<(TransactionDigest, WaitForEffectsResponse)>>;

    fn get_chain_identifier(&self) -> ChainIdentifier;
}

// TODO: Eventually remove this proxy because we shouldn't rely on validators to read objects.
pub struct LocalValidatorAggregatorProxy {
    td: Arc<TransactionDriver<NetworkAuthorityClient>>,
    committee: Committee,
    clients: BTreeMap<AuthorityName, NetworkAuthorityClient>,
    chain_identifier: ChainIdentifier,
}

impl LocalValidatorAggregatorProxy {
    pub async fn from_genesis(
        genesis: &Genesis,
        reconfig_fullnode_rpc_url: &str,
        metrics: &BenchmarkProxyMetrics,
    ) -> Self {
        let (aggregator, clients) = AuthorityAggregatorBuilder::from_genesis(genesis)
            .with_safe_client_metrics_base(metrics.safe_client_metrics_base.clone())
            .build_network_clients();
        let committee = genesis.committee();
        let chain_identifier = ChainIdentifier::from(*genesis.checkpoint().digest());
        Self::new_impl(
            aggregator,
            reconfig_fullnode_rpc_url,
            clients,
            committee,
            chain_identifier,
            metrics,
        )
        .await
    }

    async fn new_impl(
        aggregator: AuthorityAggregator<NetworkAuthorityClient>,
        reconfig_fullnode_rpc_url: &str,
        clients: BTreeMap<AuthorityName, NetworkAuthorityClient>,
        committee: Committee,
        chain_identifier: ChainIdentifier,
        metrics: &BenchmarkProxyMetrics,
    ) -> Self {
        let (aggregator, reconfig_observer): (
            Arc<_>,
            Arc<dyn ReconfigObserver<NetworkAuthorityClient> + Sync + Send>,
        ) = {
            info!(
                "Using FullNodeReconfigObserver: {:?}",
                reconfig_fullnode_rpc_url
            );
            let committee_store = aggregator.clone_committee_store();
            let reconfig_observer = Arc::new(
                FullNodeReconfigObserver::new(
                    reconfig_fullnode_rpc_url,
                    committee_store,
                    aggregator.safe_client_metrics_base.clone(),
                )
                .await,
            );
            (Arc::new(aggregator), reconfig_observer)
        };

        // For benchmark, pass None to use default validator client monitor config
        let td = TransactionDriver::new(
            aggregator,
            reconfig_observer,
            metrics.transaction_driver_metrics.clone(),
            None,
            metrics.client_metrics.clone(),
        );
        Self {
            td,
            clients,
            committee,
            chain_identifier,
        }
    }

    // Submit transaction block using Transaction Driver
    async fn submit_transaction_block(&self, tx: Transaction) -> anyhow::Result<ExecutionEffects> {
        let response = self
            .td
            .drive_transaction(
                SubmitTxRequest::new_transaction(tx.clone()),
                SubmitTransactionOptions::default(),
                Some(Duration::from_secs(60)),
            )
            .await?;
        Ok(ExecutionEffects::FinalizedTransactionEffects(
            response.effects,
            response.events.unwrap_or_default(),
        ))
    }
}

#[async_trait]
impl ValidatorProxy for LocalValidatorAggregatorProxy {
    async fn get_sui_address_balance(&self, _: SuiAddress) -> Result<u64, anyhow::Error> {
        unimplemented!("Not available for LocalValidatorAggregatorProxy");
    }

    async fn get_object(&self, object_id: ObjectID) -> Result<Object, anyhow::Error> {
        let auth_agg = self.td.authority_aggregator().load();
        Ok(auth_agg
            .get_latest_object_version_for_testing(object_id)
            .await?)
    }

    async fn get_owned_objects(
        &self,
        _account_address: SuiAddress,
    ) -> Result<Vec<(u64, Object)>, anyhow::Error> {
        unimplemented!("Not available for local proxy");
    }

    async fn get_latest_system_state_object(&self) -> Result<SuiSystemStateSummary, anyhow::Error> {
        let auth_agg = self.td.authority_aggregator().load();
        Ok(auth_agg
            .get_latest_system_state_object_for_testing()
            .await?
            .into_sui_system_state_summary())
    }

    async fn execute_transaction_block(
        &self,
        tx: Transaction,
    ) -> (ClientType, anyhow::Result<ExecutionEffects>) {
        let tx_digest = *tx.digest();
        debug!("Using TransactionDriver for transaction {:?}", tx_digest);
        (
            ClientType::TransactionDriver,
            self.submit_transaction_block(tx).await,
        )
    }

    fn clone_committee(&self) -> Arc<Committee> {
        self.td.authority_aggregator().load().committee.clone()
    }

    fn get_current_epoch(&self) -> EpochId {
        self.td.authority_aggregator().load().committee.epoch
    }

    fn clone_new(&self) -> Box<dyn ValidatorProxy + Send + Sync> {
        Box::new(Self {
            td: self.td.clone(),
            clients: self.clients.clone(),
            committee: self.committee.clone(),
            chain_identifier: self.chain_identifier,
        })
    }

    async fn get_validators(&self) -> Result<Vec<SuiAddress>, anyhow::Error> {
        let system_state = self.get_latest_system_state_object().await?;
        Ok(system_state
            .active_validators
            .iter()
            .map(|v| v.sui_address)
            .collect())
    }

    async fn execute_soft_bundle(
        &self,
        txs: Vec<Transaction>,
    ) -> anyhow::Result<Vec<(TransactionDigest, WaitForEffectsResponse)>> {
        execute_soft_bundle_with_retries(&self.td, &txs).await
    }

    fn get_chain_identifier(&self) -> ChainIdentifier {
        self.chain_identifier
    }
}

#[instrument(level = "debug", skip_all, fields(digests = ?txs.iter().map(|tx| *tx.digest()).collect::<Vec<_>>()))]
async fn execute_soft_bundle_with_retries(
    td: &TransactionDriver<NetworkAuthorityClient>,
    txs: &[Transaction],
) -> anyhow::Result<Vec<(TransactionDigest, WaitForEffectsResponse)>> {
    use sui_network::tonic::IntoRequest;

    let digests: Vec<_> = txs.iter().map(|tx| *tx.digest()).collect();

    let mut retry_cnt = 0;
    let max_retries = 10;
    let min_retry_duration = Duration::from_secs(60);
    let start = Instant::now();

    loop {
        let request = RawSubmitTxRequest {
            transactions: txs
                .iter()
                .map(|tx| bcs::to_bytes(tx).unwrap().into())
                .collect(),
            submit_type: SubmitTxType::SoftBundle.into(),
        };

        // Get a validator client - use grpc client directly for soft bundle
        // Re-select on each retry in case the previous validator is halting
        let auth_agg = td.authority_aggregator().load();
        let safe_client = auth_agg
            .authority_clients
            .values()
            .choose(&mut get_rng())
            .unwrap();

        let mut validator_client = match safe_client.authority_client().get_client_for_testing() {
            Ok(client) => client,
            Err(err) => {
                // Check if this is a retriable error before retrying
                if err.is_retryable().0
                    && (retry_cnt < max_retries || start.elapsed() < min_retry_duration)
                {
                    let delay = Duration::from_millis(rand::thread_rng().gen_range(100..1000));
                    warn!(
                        ?digests,
                        retry_cnt,
                        "Failed to get validator client with retriable error: {:?}. Sleeping for {:?} ...",
                        err,
                        delay,
                    );
                    retry_cnt += 1;
                    sleep(delay).await;
                    continue;
                }
                return Err(err.into());
            }
        };

        debug!("submitting soft bundle via grpc");

        // Submit the soft bundle via grpc
        let result = match validator_client
            .submit_transaction(request.into_request())
            .await
        {
            Ok(response) => response.into_inner(),
            Err(err) => {
                debug!("error submitting soft bundle via grpc: {:?}", err);
                // Convert tonic error to SuiError to check if retriable
                let sui_error: sui_types::error::SuiError = err.into();
                if sui_error.is_retryable().0
                    && (retry_cnt < max_retries || start.elapsed() < min_retry_duration)
                {
                    let delay = Duration::from_millis(rand::thread_rng().gen_range(100..1000));
                    warn!(
                        ?digests,
                        retry_cnt,
                        "Soft bundle submission failed with retriable error: {:?}. Sleeping for {:?} ...",
                        sui_error,
                        delay,
                    );
                    retry_cnt += 1;
                    sleep(delay).await;
                    continue;
                }
                return Err(sui_error.into());
            }
        };

        if result.results.len() != txs.len() {
            fatal!(
                "Expected {} results, got {}",
                txs.len(),
                result.results.len()
            );
        }

        // Extract consensus positions from submission results
        // Track which transactions were submitted vs rejected/executed
        // Index -> Either consensus position (for waiting) or immediate response
        enum SubmissionOutcome {
            Submitted(sui_types::messages_consensus::ConsensusPosition),
            ImmediateResponse(WaitForEffectsResponse),
        }
        let mut outcomes: Vec<SubmissionOutcome> = Vec::with_capacity(txs.len());
        let mut should_retry = false;
        let mut last_error = None;

        for raw_result in result.results.iter() {
            let submit_result: SubmitTxResult = raw_result.clone().try_into()?;
            match submit_result {
                SubmitTxResult::Submitted { consensus_position } => {
                    outcomes.push(SubmissionOutcome::Submitted(consensus_position));
                }
                SubmitTxResult::Executed {
                    effects_digest,
                    details,
                    fast_path,
                } => {
                    // Transaction was already executed - return the effects directly
                    outcomes.push(SubmissionOutcome::ImmediateResponse(
                        WaitForEffectsResponse::Executed {
                            effects_digest,
                            details,
                            fast_path,
                        },
                    ));
                }
                SubmitTxResult::Rejected { error } => {
                    // Check if this is a retriable error (e.g., ValidatorHaltedAtEpochEnd)
                    // If ANY transaction has a retriable error, retry the whole bundle
                    if error.is_retryable().0
                        && (retry_cnt < max_retries || start.elapsed() < min_retry_duration)
                    {
                        should_retry = true;
                        last_error = Some(error);
                        break;
                    }
                    // Non-retriable rejection - record as rejected response
                    outcomes.push(SubmissionOutcome::ImmediateResponse(
                        WaitForEffectsResponse::Rejected { error: Some(error) },
                    ));
                }
            }
        }

        if should_retry {
            let delay = Duration::from_millis(rand::thread_rng().gen_range(100..1000));
            warn!(
                ?digests,
                retry_cnt,
                "Soft bundle rejected with retriable error: {:?}. Sleeping for {:?} ...",
                last_error,
                delay,
            );
            retry_cnt += 1;
            sleep(delay).await;
            continue;
        }

        // Collect indices and consensus positions for transactions that need to wait for effects
        let wait_indices: Vec<usize> = outcomes
            .iter()
            .enumerate()
            .filter_map(|(i, outcome)| match outcome {
                SubmissionOutcome::Submitted(_) => Some(i),
                SubmissionOutcome::ImmediateResponse(_) => None,
            })
            .collect();

        let wait_futures: Vec<_> = wait_indices
            .iter()
            .map(|&i| {
                let consensus_position = match &outcomes[i] {
                    SubmissionOutcome::Submitted(pos) => pos,
                    _ => unreachable!(),
                };
                let request = WaitForEffectsRequest {
                    transaction_digest: Some(digests[i]),
                    consensus_position: Some(*consensus_position),
                    include_details: true,
                    ping_type: None,
                };
                safe_client.wait_for_effects(request, None)
            })
            .collect();

        let wait_responses = futures::future::join_all(wait_futures).await;

        // Build final results by combining immediate responses with waited responses
        let mut wait_response_iter = wait_responses.into_iter();
        let mut results = Vec::with_capacity(digests.len());

        for (i, outcome) in outcomes.into_iter().enumerate() {
            let response = match outcome {
                SubmissionOutcome::Submitted(_) => {
                    // Get the next waited response
                    wait_response_iter.next().unwrap()?
                }
                SubmissionOutcome::ImmediateResponse(resp) => resp,
            };
            results.push((digests[i], response));
        }

        return Ok(results);
    }
}

pub struct FullNodeProxy {
    sui_client: Client,

    // Committee and protocol config are initialized on startup and not updated on epoch changes.
    committee: Arc<Committee>,
    protocol_config: Arc<ProtocolConfig>,
    chain_identifier: ChainIdentifier,

    // TransactionDriver for soft bundle support (size > 1)
    td: Arc<TransactionDriver<NetworkAuthorityClient>>,
}

impl FullNodeProxy {
    pub async fn from_url(
        http_url: &str,
        genesis_committee: &Committee,
        metrics: &BenchmarkProxyMetrics,
    ) -> Result<Self, anyhow::Error> {
        let http_url = if http_url.starts_with("http://") || http_url.starts_with("https://") {
            http_url.to_string()
        } else {
            format!("http://{http_url}")
        };

        // Each request times out after 60s (default value)
        let sui_client = Client::new(&http_url)?;

        let committee = sui_client.get_committee(None).await?;

        let chain_identifier = sui_client.get_chain_identifier().await?;

        let protocol_config = {
            let resp = sui_client.get_protocol_config(None).await?;
            let chain = chain_identifier.chain();
            ProtocolConfig::get_for_version(resp.protocol_version().into(), chain)
        };

        // Build AuthorityAggregator and TransactionDriver for soft bundle support
        let sui_system_state = sui_client.get_system_state_summary(None).await?;
        let new_committee = sui_system_state.get_sui_committee_for_benchmarking();
        let committee_store = Arc::new(CommitteeStore::new_for_testing(genesis_committee));
        if new_committee.committee().epoch > 0 {
            committee_store.insert_new_committee(new_committee.committee())?;
        }

        let aggregator = AuthorityAggregator::new_from_committee(
            new_committee,
            Arc::new(sui_system_state.get_committee_authority_names_to_hostnames()),
            sui_system_state.reference_gas_price,
            &committee_store,
            metrics.safe_client_metrics_base.clone(),
        );

        let reconfig_observer = Arc::new(
            FullNodeReconfigObserver::new(
                &http_url,
                committee_store,
                metrics.safe_client_metrics_base.clone(),
            )
            .await,
        );

        let td = TransactionDriver::new(
            Arc::new(aggregator),
            reconfig_observer,
            metrics.transaction_driver_metrics.clone(),
            None,
            metrics.client_metrics.clone(),
        );

        Ok(Self {
            sui_client,
            committee: Arc::new(committee),
            protocol_config: Arc::new(protocol_config),
            chain_identifier,
            td,
        })
    }
}

fn is_retryable_sdk_error(err: &impl std::fmt::Debug) -> bool {
    let err_str = format!("{:?}", err);
    !(err_str.contains("Error checking transaction input objects")
        || err_str.contains("Transaction Expired")
        || err_str.contains("already locked by a different transaction")
        || err_str.contains("is not available for consumption"))
        || err_str.contains("Transaction executed but checkpoint wait timed out")
}

#[async_trait]
impl ValidatorProxy for FullNodeProxy {
    async fn get_sui_address_balance(&self, address: SuiAddress) -> Result<u64, anyhow::Error> {
        let balance = self.sui_client.get_balance(address, &GAS::type_()).await?;

        Ok(balance.address_balance())
    }

    async fn get_object(&self, object_id: ObjectID) -> Result<Object, anyhow::Error> {
        self.sui_client
            .clone()
            .get_object(object_id)
            .await
            .map_err(Into::into)
    }

    async fn get_owned_objects(
        &self,
        account_address: SuiAddress,
    ) -> Result<Vec<(u64, Object)>, anyhow::Error> {
        let objects: Vec<Object> = self
            .sui_client
            .list_owned_objects(account_address, Some(GasCoin::type_()))
            .try_collect()
            .await?;

        let mut values_objects = Vec::new();

        for object in objects {
            let gas_coin = GasCoin::try_from(&object)?;
            values_objects.push((gas_coin.value(), object));
        }

        Ok(values_objects)
    }

    async fn get_latest_system_state_object(&self) -> Result<SuiSystemStateSummary, anyhow::Error> {
        Ok(self.sui_client.get_system_state_summary(None).await?)
    }

    async fn execute_transaction_block(
        &self,
        tx: Transaction,
    ) -> (ClientType, anyhow::Result<ExecutionEffects>) {
        let tx_digest = *tx.digest();
        let start = Instant::now();
        let mut retry_cnt = 0;
        while retry_cnt < 10 || start.elapsed() < Duration::from_secs(60) {
            // Fullnode could time out after WAIT_FOR_FINALITY_TIMEOUT (30s) in TransactionOrchestrator
            // SuiClient times out after 60s
            match self
                .sui_client
                .clone()
                .execute_transaction_and_wait_for_checkpoint(&tx)
                .await
            {
                Ok(resp) => {
                    return (
                        ClientType::QuorumDriver,
                        Ok(ExecutionEffects::ExecutedTransaction(resp)),
                    );
                }
                Err(err) => {
                    if !is_retryable_sdk_error(&err) {
                        return (
                            ClientType::QuorumDriver,
                            Err(anyhow::anyhow!(
                                "Transaction {:?} failed with non-retriable error: {:?}",
                                tx_digest,
                                err
                            )),
                        );
                    }
                    let delay = Duration::from_millis(rand::thread_rng().gen_range(100..1000));
                    warn!(
                        ?tx_digest,
                        retry_cnt,
                        "Transaction failed with err: {:?}. Sleeping for {:?} ...",
                        err,
                        delay,
                    );
                    retry_cnt += 1;
                    sleep(delay).await;
                }
            }
        }
        (
            ClientType::QuorumDriver,
            Err(anyhow::anyhow!(
                "Transaction {:?} failed for {retry_cnt} times",
                tx_digest
            )),
        )
    }

    fn clone_committee(&self) -> Arc<Committee> {
        self.committee.clone()
    }

    fn get_current_epoch(&self) -> EpochId {
        self.committee.epoch
    }

    fn clone_new(&self) -> Box<dyn ValidatorProxy + Send + Sync> {
        Box::new(Self {
            sui_client: self.sui_client.clone(),
            committee: self.clone_committee(),
            protocol_config: self.protocol_config.clone(),
            chain_identifier: self.chain_identifier,
            td: self.td.clone(),
        })
    }

    async fn get_validators(&self) -> Result<Vec<SuiAddress>, anyhow::Error> {
        let validators = self
            .sui_client
            .get_system_state_summary(None)
            .await?
            .active_validators;
        Ok(validators.into_iter().map(|v| v.sui_address).collect())
    }

    async fn execute_soft_bundle(
        &self,
        txs: Vec<Transaction>,
    ) -> anyhow::Result<Vec<(TransactionDigest, WaitForEffectsResponse)>> {
        execute_soft_bundle_with_retries(&self.td, &txs).await
    }

    fn get_chain_identifier(&self) -> ChainIdentifier {
        self.chain_identifier
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum BenchMoveCallArg {
    Pure(Vec<u8>),
    Shared((ObjectID, SequenceNumber, SharedObjectMutability)),
    ImmOrOwnedObject(ObjectRef),
    ImmOrOwnedObjectVec(Vec<ObjectRef>),
    SharedObjectVec(Vec<(ObjectID, SequenceNumber, bool)>),
}

impl From<bool> for BenchMoveCallArg {
    fn from(b: bool) -> Self {
        // unwrap safe because every u8 value is BCS-serializable
        BenchMoveCallArg::Pure(bcs::to_bytes(&b).unwrap())
    }
}

impl From<u8> for BenchMoveCallArg {
    fn from(n: u8) -> Self {
        // unwrap safe because every u8 value is BCS-serializable
        BenchMoveCallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<u16> for BenchMoveCallArg {
    fn from(n: u16) -> Self {
        // unwrap safe because every u16 value is BCS-serializable
        BenchMoveCallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<u32> for BenchMoveCallArg {
    fn from(n: u32) -> Self {
        // unwrap safe because every u32 value is BCS-serializable
        BenchMoveCallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<u64> for BenchMoveCallArg {
    fn from(n: u64) -> Self {
        // unwrap safe because every u64 value is BCS-serializable
        BenchMoveCallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<u128> for BenchMoveCallArg {
    fn from(n: u128) -> Self {
        // unwrap safe because every u128 value is BCS-serializable
        BenchMoveCallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<&Vec<u8>> for BenchMoveCallArg {
    fn from(v: &Vec<u8>) -> Self {
        // unwrap safe because every vec<u8> value is BCS-serializable
        BenchMoveCallArg::Pure(bcs::to_bytes(v).unwrap())
    }
}

impl From<ObjectRef> for BenchMoveCallArg {
    fn from(obj: ObjectRef) -> Self {
        BenchMoveCallArg::ImmOrOwnedObject(obj)
    }
}

impl From<CallArg> for BenchMoveCallArg {
    fn from(ca: CallArg) -> Self {
        match ca {
            CallArg::Pure(p) => BenchMoveCallArg::Pure(p),
            CallArg::Object(obj) => match obj {
                ObjectArg::ImmOrOwnedObject(imo) => BenchMoveCallArg::ImmOrOwnedObject(imo),
                ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutability,
                } => BenchMoveCallArg::Shared((id, initial_shared_version, mutability)),
                ObjectArg::Receiving(_) => {
                    unimplemented!("Receiving is not supported for benchmarks")
                }
            },
            CallArg::FundsWithdrawal(_) => {
                // TODO(address-balances): Support FundsWithdrawal in benchmarks.
                todo!("FundsWithdrawal is not supported for benchmarks")
            }
        }
    }
}

/// Convert MoveCallArg to Vector of Argument for PT
pub fn convert_move_call_args(
    args: &[BenchMoveCallArg],
    pt_builder: &mut ProgrammableTransactionBuilder,
) -> Vec<Argument> {
    args.iter()
        .map(|arg| match arg {
            BenchMoveCallArg::Pure(bytes) => {
                pt_builder.input(CallArg::Pure(bytes.clone())).unwrap()
            }
            BenchMoveCallArg::Shared((id, initial_shared_version, mutability)) => pt_builder
                .input(CallArg::Object(ObjectArg::SharedObject {
                    id: *id,
                    initial_shared_version: *initial_shared_version,
                    mutability: *mutability,
                }))
                .unwrap(),
            BenchMoveCallArg::ImmOrOwnedObject(obj_ref) => {
                pt_builder.input((*obj_ref).into()).unwrap()
            }
            BenchMoveCallArg::ImmOrOwnedObjectVec(obj_refs) => pt_builder
                .make_obj_vec(obj_refs.iter().map(|q| ObjectArg::ImmOrOwnedObject(*q)))
                .unwrap(),
            BenchMoveCallArg::SharedObjectVec(obj_refs) => pt_builder
                .make_obj_vec(
                    obj_refs
                        .iter()
                        .map(
                            |(id, initial_shared_version, mutable)| ObjectArg::SharedObject {
                                id: *id,
                                initial_shared_version: *initial_shared_version,
                                mutability: if *mutable {
                                    SharedObjectMutability::Mutable
                                } else {
                                    SharedObjectMutability::Immutable
                                },
                            },
                        ),
                )
                .unwrap(),
        })
        .collect()
}
