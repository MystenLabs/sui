// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::bail;
use async_trait::async_trait;
use fullnode_reconfig_observer::FullNodeReconfigObserver;
use mysten_common::{fatal, random::get_rng};
use prometheus::Registry;
use rand::{Rng, seq::IteratorRandom};
use sui_config::genesis::Genesis;
use sui_core::{
    authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder},
    authority_client::NetworkAuthorityClient,
    epoch::committee_store::CommitteeStore,
    safe_client::SafeClientMetricsBase,
    transaction_driver::{
        SubmitTransactionOptions, TransactionDriver, TransactionDriverMetrics,
        reconfig_observer::ReconfigObserver,
    },
    validator_client_monitor::ValidatorClientMetrics,
};
use sui_json_rpc_types::{
    CheckpointId, SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery,
    SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions,
};
use sui_protocol_config::ProtocolConfig;
use sui_sdk::{SuiClient, SuiClientBuilder, error::Error as SuiSdkError};
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
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
use tokio::time::sleep;
use tracing::{debug, info, instrument, warn};

use crate::drivers::bench_driver::ClientType;

pub mod bank;
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
    SuiTransactionBlockEffects(SuiTransactionBlockEffects),
}

impl ExecutionEffects {
    pub fn digest(&self) -> TransactionDigest {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                *effects.data().transaction_digest()
            }
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => {
                *sui_tx_effects.transaction_digest()
            }
        }
    }

    pub fn mutated(&self) -> Vec<(ObjectRef, Owner)> {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                effects.data().mutated().to_vec()
            }
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => sui_tx_effects
                .mutated()
                .iter()
                .map(|refe| (refe.reference.to_object_ref(), refe.owner.clone()))
                .collect(),
        }
    }

    pub fn created(&self) -> Vec<(ObjectRef, Owner)> {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => effects.data().created(),
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => sui_tx_effects
                .created()
                .iter()
                .map(|refe| (refe.reference.to_object_ref(), refe.owner.clone()))
                .collect(),
        }
    }

    pub fn deleted(&self) -> Vec<ObjectRef> {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                effects.data().deleted().to_vec()
            }
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => sui_tx_effects
                .deleted()
                .iter()
                .map(|refe| refe.to_object_ref())
                .collect(),
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
            ExecutionEffects::SuiTransactionBlockEffects(_) => None,
        }
    }

    pub fn gas_object(&self) -> (ObjectRef, Owner) {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                effects.data().gas_object()
            }
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => {
                let refe = &sui_tx_effects.gas_object();
                (refe.reference.to_object_ref(), refe.owner.clone())
            }
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
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => {
                sui_tx_effects.status().is_ok()
            }
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
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => {
                let status = format!("{}", sui_tx_effects.status());
                status.contains("ExecutionCancelledDueToSharedObjectCongestion")
            }
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
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => {
                let status = format!("{}", sui_tx_effects.status());
                status.contains("InsufficientFundsForWithdraw")
            }
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
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => {
                let status = format!("{}", sui_tx_effects.status());
                status.contains("VMVerificationOrDeserializationError")
                    || status.contains("VMInvariantViolation")
                    || status.contains("FunctionNotFound")
                    || status.contains("ArityMismatch")
                    || status.contains("TypeArityMismatch")
                    || status.contains("NonEntryFunctionInvoked")
                    || status.contains("CommandArgumentError")
                    || status.contains("TypeArgumentError")
                    || status.contains("UnusedValueWithoutDrop")
                    || status.contains("InvalidPublicFunctionReturnType")
                    || status.contains("InvalidTransferObject")
            }
        }
    }

    pub fn status(&self) -> String {
        match self {
            ExecutionEffects::FinalizedTransactionEffects(effects, ..) => {
                format!("{:#?}", effects.data().status())
            }
            ExecutionEffects::SuiTransactionBlockEffects(sui_tx_effects) => {
                format!("{:#?}", sui_tx_effects.status())
            }
        }
    }

    pub fn gas_cost_summary(&self) -> GasCostSummary {
        match self {
            crate::ExecutionEffects::FinalizedTransactionEffects(a, _) => {
                a.data().gas_cost_summary().clone()
            }
            crate::ExecutionEffects::SuiTransactionBlockEffects(b) => {
                std::convert::Into::<GasCostSummary>::into(b.gas_cost_summary().clone())
            }
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
        registry: &Registry,
        reconfig_fullnode_rpc_url: &str,
    ) -> Self {
        let (aggregator, clients) = AuthorityAggregatorBuilder::from_genesis(genesis)
            .with_registry(registry)
            .build_network_clients();
        let committee = genesis.committee().unwrap();
        let chain_identifier = ChainIdentifier::from(*genesis.checkpoint().digest());
        Self::new_impl(
            aggregator,
            registry,
            reconfig_fullnode_rpc_url,
            clients,
            committee,
            chain_identifier,
        )
        .await
    }

    async fn new_impl(
        aggregator: AuthorityAggregator<NetworkAuthorityClient>,
        registry: &Registry,
        reconfig_fullnode_rpc_url: &str,
        clients: BTreeMap<AuthorityName, NetworkAuthorityClient>,
        committee: Committee,
        chain_identifier: ChainIdentifier,
    ) -> Self {
        let transaction_driver_metrics = Arc::new(TransactionDriverMetrics::new(registry));
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

        let client_metrics = Arc::new(ValidatorClientMetrics::new(registry));

        // For benchmark, pass None to use default validator client monitor config
        let td = TransactionDriver::new(
            aggregator,
            reconfig_observer,
            transaction_driver_metrics,
            None,
            client_metrics,
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
    sui_client: SuiClient,

    // Committee and protocol config are initialized on startup and not updated on epoch changes.
    committee: Arc<Committee>,
    protocol_config: Arc<ProtocolConfig>,
    chain_identifier: ChainIdentifier,

    // TransactionDriver for soft bundle support (size > 1)
    td: Arc<TransactionDriver<NetworkAuthorityClient>>,
}

impl FullNodeProxy {
    pub async fn from_url(http_url: &str, registry: &Registry) -> Result<Self, anyhow::Error> {
        let http_url = if http_url.starts_with("http://") || http_url.starts_with("https://") {
            http_url.to_string()
        } else {
            format!("http://{http_url}")
        };

        // Each request times out after 60s (default value)
        let sui_client = SuiClientBuilder::default()
            .max_concurrent_requests(500_000)
            .build(&http_url)
            .await?;

        let committee = {
            let resp = sui_client.read_api().get_committee_info(None).await?;
            let epoch = resp.epoch;
            let committee_map = resp.validators.into_iter().collect();
            Committee::new(epoch, committee_map)
        };

        let chain_identifier = {
            let genesis = sui_client
                .read_api()
                .get_checkpoint(CheckpointId::SequenceNumber(0))
                .await?;
            ChainIdentifier::from(genesis.digest)
        };

        let protocol_config = {
            let resp = sui_client.read_api().get_protocol_config(None).await?;
            let chain = chain_identifier.chain();
            ProtocolConfig::get_for_version(resp.protocol_version, chain)
        };

        // Build AuthorityAggregator and TransactionDriver for soft bundle support
        let sui_system_state = sui_client
            .governance_api()
            .get_latest_sui_system_state()
            .await?;
        let new_committee = sui_system_state.get_sui_committee_for_benchmarking();
        let committee_store = Arc::new(CommitteeStore::new_for_testing(new_committee.committee()));
        let safe_client_metrics_base = SafeClientMetricsBase::new(registry);

        let aggregator = AuthorityAggregator::new_from_committee(
            new_committee,
            Arc::new(sui_system_state.get_committee_authority_names_to_hostnames()),
            sui_system_state.reference_gas_price,
            &committee_store,
            safe_client_metrics_base.clone(),
        );

        let transaction_driver_metrics = Arc::new(TransactionDriverMetrics::new(registry));
        let reconfig_observer = Arc::new(
            FullNodeReconfigObserver::new(&http_url, committee_store, safe_client_metrics_base)
                .await,
        );

        let client_metrics = Arc::new(ValidatorClientMetrics::new(registry));
        let td = TransactionDriver::new(
            Arc::new(aggregator),
            reconfig_observer,
            transaction_driver_metrics,
            None,
            client_metrics,
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

fn is_retryable_sdk_error(err: &SuiSdkError) -> bool {
    let err_str = format!("{:?}", err);
    !(err_str.contains("Error checking transaction input objects")
        || err_str.contains("Transaction Expired")
        || err_str.contains("already locked by a different transaction")
        || err_str.contains("is not available for consumption"))
}

#[async_trait]
impl ValidatorProxy for FullNodeProxy {
    async fn get_sui_address_balance(&self, address: SuiAddress) -> Result<u64, anyhow::Error> {
        let response = self
            .sui_client
            .coin_read_api()
            .get_balance(address, None)
            .await?;

        Ok(u64::try_from(response.funds_in_address_balance).unwrap())
    }

    async fn get_object(&self, object_id: ObjectID) -> Result<Object, anyhow::Error> {
        let response = self
            .sui_client
            .read_api()
            .get_object_with_options(object_id, SuiObjectDataOptions::bcs_lossless())
            .await?;

        if let Some(sui_object) = response.data {
            sui_object.try_into_object(&self.protocol_config)
        } else if let Some(error) = response.error {
            bail!("Error getting object {:?}: {}", object_id, error)
        } else {
            bail!("Object {:?} not found and no error provided", object_id)
        }
    }

    async fn get_owned_objects(
        &self,
        account_address: SuiAddress,
    ) -> Result<Vec<(u64, Object)>, anyhow::Error> {
        let mut objects: Vec<SuiObjectResponse> = Vec::new();
        let mut cursor = None;
        loop {
            let response = self
                .sui_client
                .read_api()
                .get_owned_objects(
                    account_address,
                    Some(SuiObjectResponseQuery::new_with_options(
                        SuiObjectDataOptions::bcs_lossless(),
                    )),
                    cursor,
                    None,
                )
                .await?;

            objects.extend(response.data);

            if response.has_next_page {
                cursor = response.next_cursor;
            } else {
                break;
            }
        }

        let mut values_objects = Vec::new();

        for object in objects {
            let o = object.data;
            if let Some(o) = o {
                let temp: Object = o.clone().try_into_object(&self.protocol_config)?;
                let gas_coin = GasCoin::try_from(&temp)?;
                values_objects.push((
                    gas_coin.value(),
                    o.clone().try_into_object(&self.protocol_config)?,
                ));
            }
        }

        Ok(values_objects)
    }

    async fn get_latest_system_state_object(&self) -> Result<SuiSystemStateSummary, anyhow::Error> {
        Ok(self
            .sui_client
            .governance_api()
            .get_latest_sui_system_state()
            .await?)
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
                .quorum_driver_api()
                .execute_transaction_block(
                    tx.clone(),
                    SuiTransactionBlockResponseOptions::new().with_effects(),
                    None,
                )
                .await
            {
                Ok(resp) => {
                    return (
                        ClientType::QuorumDriver,
                        Ok(ExecutionEffects::SuiTransactionBlockEffects(
                            resp.effects.expect("effects field should not be None"),
                        )),
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
            .governance_api()
            .get_latest_sui_system_state()
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
