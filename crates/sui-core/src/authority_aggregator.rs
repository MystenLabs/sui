// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::{
    make_authority_clients_with_timeout_config, make_network_authority_clients_with_network_config,
    AuthorityAPI, NetworkAuthorityClient,
};
use crate::safe_client::{SafeClient, SafeClientMetrics, SafeClientMetricsBase};
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use mysten_metrics::{monitored_future, spawn_monitored_task, GaugeGuard, MonitorCancellation};
use mysten_network::config::Config;
use std::convert::AsRef;
use std::net::SocketAddr;
use sui_authority_aggregation::ReduceOutput;
use sui_authority_aggregation::{quorum_map_then_reduce_with_timeout, AsyncResult};
use sui_config::genesis::Genesis;
use sui_network::{
    default_mysten_network_config, DEFAULT_CONNECT_TIMEOUT_SEC, DEFAULT_REQUEST_TIMEOUT_SEC,
};
use sui_swarm_config::network_config::NetworkConfig;
use sui_types::crypto::{AuthorityPublicKeyBytes, AuthoritySignInfo};
use sui_types::error::UserInputError;
use sui_types::fp_ensure;
use sui_types::message_envelope::Message;
use sui_types::object::Object;
use sui_types::quorum_driver_types::{GroupedErrors, QuorumDriverResponse};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::{SuiSystemState, SuiSystemStateTrait};
use sui_types::{
    base_types::*,
    committee::Committee,
    error::{SuiError, SuiResult},
    transaction::*,
};
use thiserror::Error;
use tracing::{debug, error, info, instrument, trace, trace_span, warn, Instrument};

use crate::epoch::committee_store::CommitteeStore;
use crate::stake_aggregator::{InsertResult, MultiStakeAggregator, StakeAggregator};
use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry, Histogram, IntCounter,
    IntCounterVec, IntGauge, Registry,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::string::ToString;
use std::sync::Arc;
use std::time::Duration;
use sui_types::committee::{CommitteeTrait, CommitteeWithNetworkMetadata, StakeUnit};
use sui_types::effects::{
    CertifiedTransactionEffects, SignedTransactionEffects, TransactionEffects, TransactionEvents,
    VerifiedCertifiedTransactionEffects,
};
use sui_types::messages_grpc::{
    HandleCertificateRequestV3, HandleCertificateResponseV3, LayoutGenerationOption,
    ObjectInfoRequest, TransactionInfoRequest,
};
use sui_types::messages_safe_client::PlainTransactionInfoResponse;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use tokio::time::{sleep, timeout};

pub const DEFAULT_RETRIES: usize = 4;

#[cfg(test)]
#[path = "unit_tests/authority_aggregator_tests.rs"]
pub mod authority_aggregator_tests;

#[derive(Clone)]
pub struct TimeoutConfig {
    pub pre_quorum_timeout: Duration,
    pub post_quorum_timeout: Duration,

    // Timeout used to determine when to start a second "serial" request for
    // quorum_once_with_timeout. If this is set to zero, then
    // quorum_once_with_timeout becomes completely parallelized.
    pub serial_authority_request_interval: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            pre_quorum_timeout: Duration::from_secs(60),
            post_quorum_timeout: Duration::from_secs(7),
            serial_authority_request_interval: Duration::from_millis(1000),
        }
    }
}

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone)]
pub struct AuthAggMetrics {
    pub total_tx_certificates_created: IntCounter,
    pub process_tx_errors: IntCounterVec,
    pub process_cert_errors: IntCounterVec,
    pub total_client_double_spend_attempts_detected: IntCounter,
    pub total_aggregated_err: IntCounterVec,
    pub total_rpc_err: IntCounterVec,
    pub inflight_transactions: IntGauge,
    pub inflight_certificates: IntGauge,
    pub inflight_transaction_requests: IntGauge,
    pub inflight_certificate_requests: IntGauge,

    pub cert_broadcasting_post_quorum_timeout: IntCounter,
    pub remaining_tasks_when_reaching_cert_quorum: Histogram,
    pub remaining_tasks_when_cert_broadcasting_post_quorum_timeout: Histogram,
    pub quorum_reached_without_requested_objects: IntCounter,
}

impl AuthAggMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
        Self {
            total_tx_certificates_created: register_int_counter_with_registry!(
                "total_tx_certificates_created",
                "Total number of certificates made in the authority_aggregator",
                registry,
            )
            .unwrap(),
            process_tx_errors: register_int_counter_vec_with_registry!(
                "process_tx_errors",
                "Number of errors returned from validators when processing transaction, group by validator name and error type",
                &["name","error"],
                registry,
            )
            .unwrap(),
            process_cert_errors: register_int_counter_vec_with_registry!(
                "process_cert_errors",
                "Number of errors returned from validators when processing certificate, group by validator name and error type",
                &["name", "error"],
                registry,
            )
            .unwrap(),
            total_client_double_spend_attempts_detected: register_int_counter_with_registry!(
                "total_client_double_spend_attempts_detected",
                "Total number of client double spend attempts that are detected",
                registry,
            )
            .unwrap(),
            total_aggregated_err: register_int_counter_vec_with_registry!(
                "total_aggregated_err",
                "Total number of errors returned from validators, grouped by error type",
                &["error", "tx_recoverable"],
                registry,
            )
            .unwrap(),
            total_rpc_err: register_int_counter_vec_with_registry!(
                "total_rpc_err",
                "Total number of rpc errors returned from validators, grouped by validator short name and RPC error message",
                &["name", "error_message"],
                registry,
            )
            .unwrap(),
            inflight_transactions: register_int_gauge_with_registry!(
                "auth_agg_inflight_transactions",
                "Inflight transaction gathering signatures",
                registry,
            )
            .unwrap(),
            inflight_certificates: register_int_gauge_with_registry!(
                "auth_agg_inflight_certificates",
                "Inflight certificates gathering effects",
                registry,
            )
            .unwrap(),
            inflight_transaction_requests: register_int_gauge_with_registry!(
                "auth_agg_inflight_transaction_requests",
                "Inflight handle_transaction requests",
                registry,
            )
            .unwrap(),
            inflight_certificate_requests: register_int_gauge_with_registry!(
                "auth_agg_inflight_certificate_requests",
                "Inflight handle_certificate requests",
                registry,
            )
            .unwrap(),
            cert_broadcasting_post_quorum_timeout: register_int_counter_with_registry!(
                "auth_agg_cert_broadcasting_post_quorum_timeout",
                "Total number of timeout in cert processing post quorum",
                registry,
            )
            .unwrap(),
            remaining_tasks_when_reaching_cert_quorum: register_histogram_with_registry!(
                "auth_agg_remaining_tasks_when_reaching_cert_quorum",
                "Number of remaining tasks when reaching certificate quorum",
                registry,
            ).unwrap(),
            remaining_tasks_when_cert_broadcasting_post_quorum_timeout: register_histogram_with_registry!(
                "auth_agg_remaining_tasks_when_cert_broadcasting_post_quorum_timeout",
                "Number of remaining tasks when post quorum certificate broadcasting times out",
                registry,
            ).unwrap(),
            quorum_reached_without_requested_objects: register_int_counter_with_registry!(
                "auth_agg_quorum_reached_without_requested_objects",
                "Number of times quorum was reached without getting the requested objects back from at least 1 validator",
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = prometheus::Registry::new();
        Self::new(&registry)
    }
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum AggregatorProcessTransactionError {
    #[error(
        "Failed to execute transaction on a quorum of validators due to non-retryable errors. Validator errors: {:?}",
        errors,
    )]
    FatalTransaction { errors: GroupedErrors },

    #[error(
        "Failed to execute transaction on a quorum of validators but state is still retryable. Validator errors: {:?}",
        errors
    )]
    RetryableTransaction { errors: GroupedErrors },

    #[error(
        "Failed to execute transaction on a quorum of validators due to conflicting transactions. Locked objects: {:?}. Validator errors: {:?}",
        conflicting_tx_digests,
        errors,
    )]
    FatalConflictingTransaction {
        errors: GroupedErrors,
        conflicting_tx_digests:
            BTreeMap<TransactionDigest, (Vec<(AuthorityName, ObjectRef)>, StakeUnit)>,
    },

    #[error(
        "{} of the validators by stake are overloaded with transactions pending execution. Validator errors: {:?}",
        overloaded_stake,
        errors
    )]
    SystemOverload {
        overloaded_stake: StakeUnit,
        errors: GroupedErrors,
    },

    #[error("Transaction is already finalized but with different user signatures")]
    TxAlreadyFinalizedWithDifferentUserSignatures,

    #[error(
        "{} of the validators by stake are overloaded and requested the client to retry after {} seconds. Validator errors: {:?}",
        overload_stake,
        retry_after_secs,
        errors
    )]
    SystemOverloadRetryAfter {
        overload_stake: StakeUnit,
        errors: GroupedErrors,
        retry_after_secs: u64,
    },
}

#[derive(Error, Debug)]
pub enum AggregatorProcessCertificateError {
    #[error(
        "Failed to execute certificate on a quorum of validators. Non-retryable errors: {:?}",
        non_retryable_errors
    )]
    FatalExecuteCertificate { non_retryable_errors: GroupedErrors },

    #[error(
        "Failed to execute certificate on a quorum of validators but state is still retryable. Retryable errors: {:?}",
        retryable_errors
    )]
    RetryableExecuteCertificate { retryable_errors: GroupedErrors },
}

pub fn group_errors(errors: Vec<(SuiError, Vec<AuthorityName>, StakeUnit)>) -> GroupedErrors {
    #[allow(clippy::mutable_key_type)]
    let mut grouped_errors = HashMap::new();
    for (error, names, stake) in errors {
        let entry = grouped_errors.entry(error).or_insert((0, vec![]));
        entry.0 += stake;
        entry.1.extend(
            names
                .into_iter()
                .map(|n| n.concise_owned())
                .collect::<Vec<_>>(),
        );
    }
    grouped_errors
        .into_iter()
        .map(|(e, (s, n))| (e, s, n))
        .collect()
}

#[derive(Debug, Default)]
pub struct RetryableOverloadInfo {
    // Total stake of validators that are overloaded and request client to retry.
    pub total_stake: StakeUnit,

    // Records requested retry duration by stakes.
    pub stake_requested_retry_after: BTreeMap<Duration, StakeUnit>,
}

impl RetryableOverloadInfo {
    pub fn add_stake_retryable_overload(&mut self, stake: StakeUnit, retry_after: Duration) {
        self.total_stake += stake;
        self.stake_requested_retry_after
            .entry(retry_after)
            .and_modify(|s| *s += stake)
            .or_insert(stake);
    }

    // Gets the duration of retry requested by a quorum of validators with smallest retry durations.
    pub fn get_quorum_retry_after(
        &self,
        good_stake: StakeUnit,
        quorum_threshold: StakeUnit,
    ) -> Duration {
        if self.stake_requested_retry_after.is_empty() {
            return Duration::from_secs(0);
        }

        let mut quorum_stake = good_stake;
        for (retry_after, stake) in self.stake_requested_retry_after.iter() {
            quorum_stake += *stake;
            if quorum_stake >= quorum_threshold {
                return *retry_after;
            }
        }
        *self.stake_requested_retry_after.last_key_value().unwrap().0
    }
}

#[derive(Debug)]
struct ProcessTransactionState {
    // The list of signatures gathered at any point
    tx_signatures: StakeAggregator<AuthoritySignInfo, true>,
    effects_map: MultiStakeAggregator<TransactionEffectsDigest, TransactionEffects, true>,
    // The list of errors gathered at any point
    errors: Vec<(SuiError, Vec<AuthorityName>, StakeUnit)>,
    // This is exclusively non-retryable stake.
    non_retryable_stake: StakeUnit,
    // This includes both object and package not found sui errors.
    object_or_package_not_found_stake: StakeUnit,
    // Validators that are overloaded with txns pending execution.
    overloaded_stake: StakeUnit,
    // Validators that are overloaded and request client to retry.
    retryable_overload_info: RetryableOverloadInfo,
    // If there are conflicting transactions, we note them down to report to user.
    conflicting_tx_digests:
        BTreeMap<TransactionDigest, (Vec<(AuthorityName, ObjectRef)>, StakeUnit)>,
    // As long as none of the exit criteria are met we consider the state retryable
    // 1) >= 2f+1 signatures
    // 2) >= f+1 non-retryable errors
    // 3) >= 2f+1 object not found errors
    retryable: bool,
    tx_finalized_with_different_user_sig: bool,
}

impl ProcessTransactionState {
    pub fn record_conflicting_transaction_if_any(
        &mut self,
        validator_name: AuthorityName,
        weight: StakeUnit,
        err: &SuiError,
    ) {
        if let SuiError::ObjectLockConflict {
            obj_ref,
            pending_transaction: transaction,
        } = err
        {
            let (lock_records, total_stake) = self
                .conflicting_tx_digests
                .entry(*transaction)
                .or_insert((Vec::new(), 0));
            lock_records.push((validator_name, *obj_ref));
            *total_stake += weight;
        }
    }

    pub fn check_if_error_indicates_tx_finalized_with_different_user_sig(
        &self,
        validity_threshold: StakeUnit,
    ) -> bool {
        // In some edge cases, the client may send the same transaction multiple times but with different user signatures.
        // When this happens, the "minority" tx will fail in safe_client because the certificate verification would fail
        // and return Sui::FailedToVerifyTxCertWithExecutedEffects.
        // Here, we check if there are f+1 validators return this error. If so, the transaction is already finalized
        // with a different set of user signatures. It's not trivial to return the results of that successful transaction
        // because we don't want fullnode to store the transaction with non-canonical user signatures. Given that this is
        // very rare, we simply return an error here.
        let invalid_sig_stake: StakeUnit = self
            .errors
            .iter()
            .filter_map(|(e, _, stake)| {
                if matches!(e, SuiError::FailedToVerifyTxCertWithExecutedEffects { .. }) {
                    Some(stake)
                } else {
                    None
                }
            })
            .sum();
        invalid_sig_stake >= validity_threshold
    }
}

struct ProcessCertificateState {
    // Different authorities could return different effects.  We want at least one effect to come
    // from 2f+1 authorities, which meets quorum and can be considered the approved effect.
    // The map here allows us to count the stake for each unique effect.
    effects_map:
        MultiStakeAggregator<(EpochId, TransactionEffectsDigest), TransactionEffects, true>,
    non_retryable_stake: StakeUnit,
    non_retryable_errors: Vec<(SuiError, Vec<AuthorityName>, StakeUnit)>,
    retryable_errors: Vec<(SuiError, Vec<AuthorityName>, StakeUnit)>,
    // As long as none of the exit criteria are met we consider the state retryable
    // 1) >= 2f+1 signatures
    // 2) >= f+1 non-retryable errors
    retryable: bool,

    // collection of extended data returned from the validators.
    // Not all validators will be asked to return this data so we need to hold onto it when one
    // validator has provided it
    events: Option<TransactionEvents>,
    input_objects: Option<Vec<Object>>,
    output_objects: Option<Vec<Object>>,
    auxiliary_data: Option<Vec<u8>>,
    request: HandleCertificateRequestV3,
}

#[derive(Debug)]
pub enum ProcessTransactionResult {
    Certified {
        certificate: CertifiedTransaction,
        /// Whether this certificate is newly created by aggregating 2f+1 signatures.
        /// If a validator returned a cert directly, this will be false.
        /// This is used to inform the quorum driver, which could make better decisions on telemetry
        /// such as settlement latency.
        newly_formed: bool,
    },
    Executed(VerifiedCertifiedTransactionEffects, TransactionEvents),
}

impl ProcessTransactionResult {
    pub fn into_cert_for_testing(self) -> CertifiedTransaction {
        match self {
            Self::Certified { certificate, .. } => certificate,
            Self::Executed(..) => panic!("Wrong type"),
        }
    }

    pub fn into_effects_for_testing(self) -> VerifiedCertifiedTransactionEffects {
        match self {
            Self::Certified { .. } => panic!("Wrong type"),
            Self::Executed(effects, ..) => effects,
        }
    }
}

#[derive(Clone)]
pub struct AuthorityAggregator<A: Clone> {
    /// Our Sui committee.
    pub committee: Arc<Committee>,
    /// For more human readable metrics reporting.
    /// It's OK for this map to be empty or missing validators, it then defaults
    /// to use concise validator public keys.
    pub validator_display_names: Arc<HashMap<AuthorityName, String>>,
    /// How to talk to this committee.
    pub authority_clients: Arc<BTreeMap<AuthorityName, Arc<SafeClient<A>>>>,
    /// Metrics
    pub metrics: Arc<AuthAggMetrics>,
    /// Metric base for the purpose of creating new safe clients during reconfiguration.
    pub safe_client_metrics_base: SafeClientMetricsBase,
    pub timeouts: TimeoutConfig,
    /// Store here for clone during re-config.
    pub committee_store: Arc<CommitteeStore>,
}

impl<A: Clone> AuthorityAggregator<A> {
    pub fn new(
        committee: Committee,
        committee_store: Arc<CommitteeStore>,
        authority_clients: BTreeMap<AuthorityName, A>,
        safe_client_metrics_base: SafeClientMetricsBase,
        auth_agg_metrics: Arc<AuthAggMetrics>,
        validator_display_names: Arc<HashMap<AuthorityName, String>>,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            committee: Arc::new(committee),
            authority_clients: create_safe_clients(
                authority_clients,
                &committee_store,
                &safe_client_metrics_base,
            ),
            metrics: auth_agg_metrics,
            safe_client_metrics_base,
            timeouts,
            committee_store,
            validator_display_names,
        }
    }

    /// This function recreates AuthorityAggregator with the given committee.
    /// It also updates committee store which impacts other of its references.
    /// When disallow_missing_intermediate_committees is true, it requires the
    /// new committee needs to be current epoch + 1.
    /// The function could be used along with `reconfig_from_genesis` to fill in
    /// all previous epoch's committee info.
    pub fn recreate_with_net_addresses(
        &self,
        committee: CommitteeWithNetworkMetadata,
        network_config: &Config,
        disallow_missing_intermediate_committees: bool,
    ) -> SuiResult<AuthorityAggregator<NetworkAuthorityClient>> {
        let network_clients =
            make_network_authority_clients_with_network_config(&committee, network_config);

        let safe_clients = network_clients
            .into_iter()
            .map(|(name, api)| {
                (
                    name,
                    Arc::new(SafeClient::new(
                        api,
                        self.committee_store.clone(),
                        name,
                        SafeClientMetrics::new(&self.safe_client_metrics_base, name),
                    )),
                )
            })
            .collect::<BTreeMap<_, _>>();

        // TODO: It's likely safer to do the following operations atomically, in case this function
        // gets called from different threads. It cannot happen today, but worth the caution.
        let new_committee = committee.committee().clone();
        if disallow_missing_intermediate_committees {
            fp_ensure!(
                self.committee.epoch + 1 == new_committee.epoch,
                SuiError::AdvanceEpochError {
                    error: format!(
                        "Trying to advance from epoch {} to epoch {}",
                        self.committee.epoch, new_committee.epoch
                    )
                }
            );
        }
        // This call may return error if this committee is already inserted,
        // which is fine. We should continue to construct the new aggregator.
        // This is because there may be multiple AuthorityAggregators
        // or its containers (e.g. Quorum Drivers)  share the same committee
        // store and all of them need to reconfigure.
        let _ = self.committee_store.insert_new_committee(&new_committee);
        Ok(AuthorityAggregator {
            committee: Arc::new(new_committee),
            authority_clients: Arc::new(safe_clients),
            metrics: self.metrics.clone(),
            timeouts: self.timeouts.clone(),
            safe_client_metrics_base: self.safe_client_metrics_base.clone(),
            committee_store: self.committee_store.clone(),
            validator_display_names: Arc::new(HashMap::new()),
        })
    }

    pub fn get_client(&self, name: &AuthorityName) -> Option<&Arc<SafeClient<A>>> {
        self.authority_clients.get(name)
    }

    pub fn clone_client_test_only(&self, name: &AuthorityName) -> Arc<SafeClient<A>>
    where
        A: Clone,
    {
        self.authority_clients[name].clone()
    }

    pub fn clone_committee_store(&self) -> Arc<CommitteeStore> {
        self.committee_store.clone()
    }

    pub fn clone_inner_committee_test_only(&self) -> Committee {
        (*self.committee).clone()
    }

    pub fn clone_inner_clients_test_only(&self) -> BTreeMap<AuthorityName, SafeClient<A>> {
        (*self.authority_clients)
            .clone()
            .into_iter()
            .map(|(k, v)| (k, (*v).clone()))
            .collect()
    }
}

fn create_safe_clients<A: Clone>(
    authority_clients: BTreeMap<AuthorityName, A>,
    committee_store: &Arc<CommitteeStore>,
    safe_client_metrics_base: &SafeClientMetricsBase,
) -> Arc<BTreeMap<AuthorityName, Arc<SafeClient<A>>>> {
    Arc::new(
        authority_clients
            .into_iter()
            .map(|(name, api)| {
                (
                    name,
                    Arc::new(SafeClient::new(
                        api,
                        committee_store.clone(),
                        name,
                        SafeClientMetrics::new(safe_client_metrics_base, name),
                    )),
                )
            })
            .collect(),
    )
}

impl AuthorityAggregator<NetworkAuthorityClient> {
    /// Create a new network authority aggregator by reading the committee and network addresses
    /// information from the given epoch start system state.
    pub fn new_from_epoch_start_state(
        epoch_start_state: &EpochStartSystemState,
        committee_store: &Arc<CommitteeStore>,
        safe_client_metrics_base: SafeClientMetricsBase,
        auth_agg_metrics: Arc<AuthAggMetrics>,
    ) -> Self {
        let committee = epoch_start_state.get_sui_committee_with_network_metadata();
        let validator_display_names = epoch_start_state.get_authority_names_to_hostnames();
        Self::new_from_committee(
            committee,
            committee_store,
            safe_client_metrics_base,
            auth_agg_metrics,
            Arc::new(validator_display_names),
        )
    }

    /// Create a new AuthorityAggregator using information from the given epoch start system state.
    /// This is typically used during reconfiguration to create a new AuthorityAggregator with the
    /// new committee and network addresses.
    pub fn recreate_with_new_epoch_start_state(
        &self,
        epoch_start_state: &EpochStartSystemState,
    ) -> Self {
        Self::new_from_epoch_start_state(
            epoch_start_state,
            &self.committee_store,
            self.safe_client_metrics_base.clone(),
            self.metrics.clone(),
        )
    }

    pub fn new_from_committee(
        committee: CommitteeWithNetworkMetadata,
        committee_store: &Arc<CommitteeStore>,
        safe_client_metrics_base: SafeClientMetricsBase,
        auth_agg_metrics: Arc<AuthAggMetrics>,
        validator_display_names: Arc<HashMap<AuthorityName, String>>,
    ) -> Self {
        let net_config = default_mysten_network_config();
        let authority_clients =
            make_network_authority_clients_with_network_config(&committee, &net_config);
        Self::new(
            committee.committee().clone(),
            committee_store.clone(),
            authority_clients,
            safe_client_metrics_base,
            auth_agg_metrics,
            validator_display_names,
            Default::default(),
        )
    }
}

impl<A> AuthorityAggregator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Repeatedly calls the provided closure on a randomly selected validator until it succeeds.
    // Once all validators have been attempted, starts over at the beginning. Intended for cases
    // that must eventually succeed as long as the network is up (or comes back up) eventually.
    async fn quorum_once_inner<'a, S, FMap>(
        &'a self,
        // try these authorities first
        preferences: Option<&BTreeSet<AuthorityName>>,
        // only attempt from these authorities.
        restrict_to: Option<&BTreeSet<AuthorityName>>,
        // The async function used to apply to each authority. It takes an authority name,
        // and authority client parameter and returns a Result<V>.
        map_each_authority: FMap,
        timeout_each_authority: Duration,
        authority_errors: &mut HashMap<AuthorityName, SuiError>,
    ) -> Result<S, SuiError>
    where
        FMap: Fn(AuthorityName, Arc<SafeClient<A>>) -> AsyncResult<'a, S, SuiError>
            + Send
            + Clone
            + 'a,
        S: Send,
    {
        let start = tokio::time::Instant::now();
        let mut delay = Duration::from_secs(1);
        loop {
            let authorities_shuffled = self.committee.shuffle_by_stake(preferences, restrict_to);
            let mut authorities_shuffled = authorities_shuffled.iter();

            type RequestResult<S> = Result<Result<S, SuiError>, tokio::time::error::Elapsed>;

            enum Event<S> {
                StartNext,
                Request(AuthorityName, RequestResult<S>),
            }

            let mut futures = FuturesUnordered::<BoxFuture<'a, Event<S>>>::new();

            let start_req = |name: AuthorityName, client: Arc<SafeClient<A>>| {
                let map_each_authority = map_each_authority.clone();
                Box::pin(monitored_future!(async move {
                    trace!(name=?name.concise(), now = ?tokio::time::Instant::now() - start, "new request");
                    let map = map_each_authority(name, client);
                    Event::Request(name, timeout(timeout_each_authority, map).await)
                }))
            };

            let schedule_next = || {
                let delay = self.timeouts.serial_authority_request_interval;
                Box::pin(monitored_future!(async move {
                    sleep(delay).await;
                    Event::StartNext
                }))
            };

            // This process is intended to minimize latency in the face of unreliable authorities,
            // without creating undue load on authorities.
            //
            // The fastest possible process from the
            // client's point of view would simply be to issue a concurrent request to every
            // authority and then take the winner - this would create unnecessary load on
            // authorities.
            //
            // The most efficient process from the network's point of view is to do one request at
            // a time, however if the first validator that the client contacts is unavailable or
            // slow, the client must wait for the serial_authority_request_interval period to elapse
            // before starting its next request.
            //
            // So, this process is designed as a compromise between these two extremes.
            // - We start one request, and schedule another request to begin after
            //   serial_authority_request_interval.
            // - Whenever a request finishes, if it succeeded, we return. if it failed, we start a
            //   new request.
            // - If serial_authority_request_interval elapses, we begin a new request even if the
            //   previous one is not finished, and schedule another future request.

            let name = authorities_shuffled.next().ok_or_else(|| {
                error!(
                    ?preferences,
                    ?restrict_to,
                    "Available authorities list is empty."
                );
                SuiError::from("Available authorities list is empty")
            })?;
            futures.push(start_req(*name, self.authority_clients[name].clone()));
            futures.push(schedule_next());

            while let Some(res) = futures.next().await {
                match res {
                    Event::StartNext => {
                        trace!(now = ?tokio::time::Instant::now() - start, "eagerly beginning next request");
                        futures.push(schedule_next());
                    }
                    Event::Request(name, res) => {
                        match res {
                            // timeout
                            Err(_) => {
                                debug!(name=?name.concise(), "authority request timed out");
                                authority_errors.insert(name, SuiError::TimeoutError);
                            }
                            // request completed
                            Ok(inner_res) => {
                                trace!(name=?name.concise(), now = ?tokio::time::Instant::now() - start,
                                       "request completed successfully");
                                match inner_res {
                                    Err(e) => authority_errors.insert(name, e),
                                    Ok(res) => return Ok(res),
                                };
                            }
                        };
                    }
                }

                if let Some(next_authority) = authorities_shuffled.next() {
                    futures.push(start_req(
                        *next_authority,
                        self.authority_clients[next_authority].clone(),
                    ));
                } else {
                    break;
                }
            }

            info!(
                ?authority_errors,
                "quorum_once_with_timeout failed on all authorities, retrying in {:?}", delay
            );
            sleep(delay).await;
            delay = std::cmp::min(delay * 2, Duration::from_secs(5 * 60));
        }
    }

    /// Like quorum_map_then_reduce_with_timeout, but for things that need only a single
    /// successful response, such as fetching a Transaction from some authority.
    /// This is intended for cases in which byzantine authorities can time out or slow-loris, but
    /// can't give a false answer, because e.g. the digest of the response is known, or a
    /// quorum-signed object such as a checkpoint has been requested.
    pub(crate) async fn quorum_once_with_timeout<'a, S, FMap>(
        &'a self,
        // try these authorities first
        preferences: Option<&BTreeSet<AuthorityName>>,
        // only attempt from these authorities.
        restrict_to: Option<&BTreeSet<AuthorityName>>,
        // The async function used to apply to each authority. It takes an authority name,
        // and authority client parameter and returns a Result<V>.
        map_each_authority: FMap,
        timeout_each_authority: Duration,
        // When to give up on the attempt entirely.
        timeout_total: Option<Duration>,
        // The behavior that authorities expect to perform, used for logging and error
        description: String,
    ) -> Result<S, SuiError>
    where
        FMap: Fn(AuthorityName, Arc<SafeClient<A>>) -> AsyncResult<'a, S, SuiError>
            + Send
            + Clone
            + 'a,
        S: Send,
    {
        let mut authority_errors = HashMap::new();

        let fut = self.quorum_once_inner(
            preferences,
            restrict_to,
            map_each_authority,
            timeout_each_authority,
            &mut authority_errors,
        );

        if let Some(t) = timeout_total {
            timeout(t, fut).await.map_err(|_timeout_error| {
                if authority_errors.is_empty() {
                    SuiError::TimeoutError
                } else {
                    SuiError::TooManyIncorrectAuthorities {
                        errors: authority_errors
                            .iter()
                            .map(|(a, b)| (*a, b.clone()))
                            .collect(),
                        action: description,
                    }
                }
            })?
        } else {
            fut.await
        }
    }

    /// Query the object with highest version number from the authorities.
    /// We stop after receiving responses from 2f+1 validators.
    /// This function is untrusted because we simply assume each response is valid and there are no
    /// byzantine validators.
    /// Because of this, this function should only be used for testing or benchmarking.
    pub async fn get_latest_object_version_for_testing(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Object> {
        #[derive(Debug, Default)]
        struct State {
            latest_object_version: Option<Object>,
            total_weight: StakeUnit,
        }
        let initial_state = State::default();
        let result = quorum_map_then_reduce_with_timeout(
                self.committee.clone(),
                self.authority_clients.clone(),
                initial_state,
                |_name, client| {
                    Box::pin(async move {
                        let request =
                            ObjectInfoRequest::latest_object_info_request(object_id, /* generate_layout */ LayoutGenerationOption::None);
                        client.handle_object_info_request(request).await
                    })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        state.total_weight += weight;
                        match result {
                            Ok(object_info) => {
                                debug!("Received object info response from validator {:?} with version: {:?}", name.concise(), object_info.object.version());
                                if state.latest_object_version.as_ref().map_or(true, |latest| {
                                    object_info.object.version() > latest.version()
                                }) {
                                    state.latest_object_version = Some(object_info.object);
                                }
                            }
                            Err(err) => {
                                debug!("Received error from validator {:?}: {:?}", name.concise(), err);
                            }
                        };
                        if state.total_weight >= self.committee.quorum_threshold() {
                            if let Some(object) = state.latest_object_version {
                                return ReduceOutput::Success(object);
                            } else {
                                return ReduceOutput::Failed(state);
                            }
                        }
                        ReduceOutput::Continue(state)
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await.map_err(|_state| SuiError::from(UserInputError::ObjectNotFound {
                object_id,
                version: None,
            }))?;
        Ok(result.0)
    }

    /// Get the latest system state object from the authorities.
    /// This function assumes all validators are honest.
    /// It should only be used for testing or benchmarking.
    pub async fn get_latest_system_state_object_for_testing(
        &self,
    ) -> anyhow::Result<SuiSystemState> {
        #[derive(Debug, Default)]
        struct State {
            latest_system_state: Option<SuiSystemState>,
            total_weight: StakeUnit,
        }
        let initial_state = State::default();
        let result = quorum_map_then_reduce_with_timeout(
            self.committee.clone(),
            self.authority_clients.clone(),
            initial_state,
            |_name, client| Box::pin(async move { client.handle_system_state_object().await }),
            |mut state, name, weight, result| {
                Box::pin(async move {
                    state.total_weight += weight;
                    match result {
                        Ok(system_state) => {
                            debug!(
                                "Received system state object from validator {:?} with epoch: {:?}",
                                name.concise(),
                                system_state.epoch()
                            );
                            if state
                                .latest_system_state
                                .as_ref()
                                .map_or(true, |latest| system_state.epoch() > latest.epoch())
                            {
                                state.latest_system_state = Some(system_state);
                            }
                        }
                        Err(err) => {
                            debug!(
                                "Received error from validator {:?}: {:?}",
                                name.concise(),
                                err
                            );
                        }
                    };
                    if state.total_weight >= self.committee.quorum_threshold() {
                        if let Some(system_state) = state.latest_system_state {
                            return ReduceOutput::Success(system_state);
                        } else {
                            return ReduceOutput::Failed(state);
                        }
                    }
                    ReduceOutput::Continue(state)
                })
            },
            // A long timeout before we hear back from a quorum
            self.timeouts.pre_quorum_timeout,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Failed to get latest system state from the authorities"))?;
        Ok(result.0)
    }

    /// Submits the transaction to a quorum of validators to make a certificate.
    #[instrument(level = "trace", skip_all)]
    pub async fn process_transaction(
        &self,
        transaction: Transaction,
        client_addr: Option<SocketAddr>,
    ) -> Result<ProcessTransactionResult, AggregatorProcessTransactionError> {
        // Now broadcast the transaction to all authorities.
        let tx_digest = transaction.digest();
        debug!(
            tx_digest = ?tx_digest,
            "Broadcasting transaction request to authorities"
        );
        trace!(
            "Transaction data: {:?}",
            transaction.data().intent_message().value
        );
        let committee = self.committee.clone();
        let state = ProcessTransactionState {
            tx_signatures: StakeAggregator::new(committee.clone()),
            effects_map: MultiStakeAggregator::new(committee.clone()),
            errors: vec![],
            object_or_package_not_found_stake: 0,
            non_retryable_stake: 0,
            overloaded_stake: 0,
            retryable_overload_info: Default::default(),
            retryable: true,
            conflicting_tx_digests: Default::default(),
            tx_finalized_with_different_user_sig: false,
        };

        let transaction_ref = &transaction;
        let validity_threshold = committee.validity_threshold();
        let quorum_threshold = committee.quorum_threshold();
        let validator_display_names = self.validator_display_names.clone();
        let result = quorum_map_then_reduce_with_timeout(
                committee.clone(),
                self.authority_clients.clone(),
                state,
                |name, client| {
                    Box::pin(
                        async move {
                            let _guard = GaugeGuard::acquire(&self.metrics.inflight_transaction_requests);
                            let concise_name = name.concise_owned();
                            client.handle_transaction(transaction_ref.clone(), client_addr)
                                .monitor_cancellation()
                                .instrument(trace_span!("handle_transaction", cancelled = false, authority =? concise_name))
                                .await
                        },
                    )
                },
                |mut state, name, weight, response| {
                    let display_name = validator_display_names.get(&name).unwrap_or(&name.concise().to_string()).clone();
                    Box::pin(async move {
                        match self.handle_process_transaction_response(
                            tx_digest, &mut state, response, name, weight,
                        ) {
                            Ok(Some(result)) => {
                                self.record_process_transaction_metrics(tx_digest, &state);
                                return ReduceOutput::Success(result);
                            }
                            Ok(None) => {},
                            Err(err) => {
                                let concise_name = name.concise();
                                debug!(?tx_digest, name=?concise_name, weight, "Error processing transaction from validator: {:?}", err);
                                self.metrics
                                    .process_tx_errors
                                    .with_label_values(&[&display_name, err.as_ref()])
                                    .inc();
                                Self::record_rpc_error_maybe(self.metrics.clone(), &display_name, &err);
                                // Record conflicting transactions if any to report to user.
                                state.record_conflicting_transaction_if_any(name, weight, &err);
                                let (retryable, categorized) = err.is_retryable();
                                if !categorized {
                                    // TODO: Should minimize possible uncategorized errors here
                                    // use ERROR for now to make them easier to spot.
                                    error!(?tx_digest, "uncategorized tx error: {err}");
                                }
                                if err.is_object_or_package_not_found() {
                                    // Special case for object not found because we can
                                    // retry if we have < 2f+1 object not found errors.
                                    // However once we reach >= 2f+1 object not found errors
                                    // we cannot retry.
                                    state.object_or_package_not_found_stake += weight;
                                }
                                else if err.is_overload() {
                                    // Special case for validator overload too. Once we have >= 2f + 1
                                    // overloaded validators we consider the system overloaded so we exit
                                    // and notify the user.
                                    // Note that currently, this overload account for
                                    //   - per object queue overload
                                    //   - consensus overload
                                    state.overloaded_stake += weight;
                                }
                                else if err.is_retryable_overload() {
                                    // Different from above overload error, retryable overload targets authority overload (entire
                                    // authority server is overload). In this case, the retry behavior is different from
                                    // above that we may perform continuous retry due to that objects may have been locked
                                    // in the validator.
                                    //
                                    // TODO: currently retryable overload and above overload error look redundant. We want to have a unified
                                    // code path to handle both overload scenarios.
                                    state.retryable_overload_info.add_stake_retryable_overload(weight, Duration::from_secs(err.retry_after_secs()));
                                }
                                else if !retryable {
                                    state.non_retryable_stake += weight;
                                }
                                state.errors.push((err, vec![name], weight));

                            }
                        };

                        let retryable_stake = self.get_retryable_stake(&state);
                        let good_stake = std::cmp::max(state.tx_signatures.total_votes(), state.effects_map.total_votes());
                        if good_stake + retryable_stake < quorum_threshold {
                            debug!(
                                tx_digest = ?tx_digest,
                                good_stake,
                                retryable_stake,
                                "No chance for any tx to get quorum, exiting. Conflicting_txes: {:?}",
                                state.conflicting_tx_digests
                            );
                            // If there is no chance for any tx to get quorum, exit.
                            state.retryable = false;
                            return ReduceOutput::Failed(state);
                        }

                        // TODO: add more comments to explain each condition.
                        if state.non_retryable_stake >= validity_threshold
                            || state.object_or_package_not_found_stake >= quorum_threshold // In normal case, object/package not found should be more than f+1
                            || state.overloaded_stake >= quorum_threshold {
                            // We have hit an exit condition, f+1 non-retryable err or 2f+1 object not found or overload,
                            // so we no longer consider the transaction state as retryable.
                            state.retryable = false;
                            ReduceOutput::Failed(state)
                        } else {
                            ReduceOutput::Continue(state)
                        }
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await;

        match result {
            Ok((result, _)) => Ok(result),
            Err(state) => {
                self.record_process_transaction_metrics(tx_digest, &state);
                let state = self.record_non_quorum_effects_maybe(tx_digest, state);
                Err(self.handle_process_transaction_error(state))
            }
        }
    }

    fn record_rpc_error_maybe(
        metrics: Arc<AuthAggMetrics>,
        display_name: &String,
        error: &SuiError,
    ) {
        if let SuiError::RpcError(_message, code) = error {
            metrics
                .total_rpc_err
                .with_label_values(&[display_name, code.as_str()])
                .inc();
        }
    }

    fn handle_process_transaction_error(
        &self,
        state: ProcessTransactionState,
    ) -> AggregatorProcessTransactionError {
        let quorum_threshold = self.committee.quorum_threshold();

        // Return system overload error if we see >= 2f + 1 overloaded stake.
        if state.overloaded_stake >= quorum_threshold {
            return AggregatorProcessTransactionError::SystemOverload {
                overloaded_stake: state.overloaded_stake,
                errors: group_errors(state.errors),
            };
        }

        if !state.retryable {
            if state.tx_finalized_with_different_user_sig
                || state.check_if_error_indicates_tx_finalized_with_different_user_sig(
                    self.committee.validity_threshold(),
                )
            {
                return AggregatorProcessTransactionError::TxAlreadyFinalizedWithDifferentUserSignatures;
            }

            // Handle conflicts first as `FatalConflictingTransaction` which is
            // more meaningful than `FatalTransaction`
            if !state.conflicting_tx_digests.is_empty() {
                let good_stake = state.tx_signatures.total_votes();
                warn!(
                    ?state.conflicting_tx_digests,
                    original_tx_stake = good_stake,
                    "Client double spend attempt detected!",
                );
                self.metrics
                    .total_client_double_spend_attempts_detected
                    .inc();
                return AggregatorProcessTransactionError::FatalConflictingTransaction {
                    errors: group_errors(state.errors),
                    conflicting_tx_digests: state.conflicting_tx_digests,
                };
            }

            return AggregatorProcessTransactionError::FatalTransaction {
                errors: group_errors(state.errors),
            };
        }

        // When state is in a retryable state and process transaction was not successful, it indicates that
        // we have heard from *all* validators. Check if any SystemOverloadRetryAfter error caused the txn
        // to fail. If so, return explicit SystemOverloadRetryAfter error for continuous retry (since objects
        // are locked in validators). If not, retry regular RetryableTransaction error.
        if state.tx_signatures.total_votes() + state.retryable_overload_info.total_stake
            >= quorum_threshold
        {
            let retry_after_secs = state
                .retryable_overload_info
                .get_quorum_retry_after(state.tx_signatures.total_votes(), quorum_threshold)
                .as_secs();
            return AggregatorProcessTransactionError::SystemOverloadRetryAfter {
                overload_stake: state.retryable_overload_info.total_stake,
                errors: group_errors(state.errors),
                retry_after_secs,
            };
        }

        // The system is not overloaded and transaction state is still retryable.
        AggregatorProcessTransactionError::RetryableTransaction {
            errors: group_errors(state.errors),
        }
    }

    fn record_process_transaction_metrics(
        &self,
        tx_digest: &TransactionDigest,
        state: &ProcessTransactionState,
    ) {
        let num_signatures = state.tx_signatures.validator_sig_count();
        let good_stake = state.tx_signatures.total_votes();
        debug!(
            ?tx_digest,
            num_errors = state.errors.iter().map(|e| e.1.len()).sum::<usize>(),
            num_unique_errors = state.errors.len(),
            ?good_stake,
            non_retryable_stake = state.non_retryable_stake,
            ?num_signatures,
            "Received signatures response from validators handle_transaction"
        );
        if !state.errors.is_empty() {
            debug!(?tx_digest, "Errors received: {:?}", state.errors);
        }
    }

    fn handle_process_transaction_response(
        &self,
        tx_digest: &TransactionDigest,
        state: &mut ProcessTransactionState,
        response: SuiResult<PlainTransactionInfoResponse>,
        name: AuthorityName,
        weight: StakeUnit,
    ) -> SuiResult<Option<ProcessTransactionResult>> {
        match response {
            Ok(PlainTransactionInfoResponse::Signed(signed)) => {
                debug!(?tx_digest, name=?name.concise(), weight, "Received signed transaction from validator handle_transaction");
                self.handle_transaction_response_with_signed(state, signed)
            }
            Ok(PlainTransactionInfoResponse::ExecutedWithCert(cert, effects, events)) => {
                debug!(?tx_digest, name=?name.concise(), weight, "Received prev certificate and effects from validator handle_transaction");
                self.handle_transaction_response_with_executed(state, Some(cert), effects, events)
            }
            Ok(PlainTransactionInfoResponse::ExecutedWithoutCert(_, effects, events)) => {
                debug!(?tx_digest, name=?name.concise(), weight, "Received prev effects from validator handle_transaction");
                self.handle_transaction_response_with_executed(state, None, effects, events)
            }
            Err(err) => Err(err),
        }
    }

    fn handle_transaction_response_with_signed(
        &self,
        state: &mut ProcessTransactionState,
        plain_tx: SignedTransaction,
    ) -> SuiResult<Option<ProcessTransactionResult>> {
        match state.tx_signatures.insert(plain_tx.clone()) {
            InsertResult::NotEnoughVotes {
                bad_votes,
                bad_authorities,
            } => {
                state.non_retryable_stake += bad_votes;
                if bad_votes > 0 {
                    state.errors.push((
                        SuiError::InvalidSignature {
                            error: "Individual signature verification failed".to_string(),
                        },
                        bad_authorities,
                        bad_votes,
                    ));
                }
                Ok(None)
            }
            InsertResult::Failed { error } => Err(error),
            InsertResult::QuorumReached(cert_sig) => {
                let certificate =
                    CertifiedTransaction::new_from_data_and_sig(plain_tx.into_data(), cert_sig);
                certificate.verify_committee_sigs_only(&self.committee)?;
                Ok(Some(ProcessTransactionResult::Certified {
                    certificate,
                    newly_formed: true,
                }))
            }
        }
    }

    fn handle_transaction_response_with_executed(
        &self,
        state: &mut ProcessTransactionState,
        certificate: Option<CertifiedTransaction>,
        plain_tx_effects: SignedTransactionEffects,
        events: TransactionEvents,
    ) -> SuiResult<Option<ProcessTransactionResult>> {
        match certificate {
            Some(certificate) if certificate.epoch() == self.committee.epoch => {
                // If we get a certificate in the same epoch, then we use it.
                // A certificate in a past epoch does not guarantee finality
                // and validators may reject to process it.
                Ok(Some(ProcessTransactionResult::Certified {
                    certificate,
                    newly_formed: false,
                }))
            }
            _ => {
                // If we get 2f+1 effects, it's a proof that the transaction
                // has already been finalized. This works because validators would re-sign effects for transactions
                // that were finalized in previous epochs.
                let digest = plain_tx_effects.data().digest();
                match state.effects_map.insert(digest, plain_tx_effects.clone()) {
                    InsertResult::NotEnoughVotes {
                        bad_votes,
                        bad_authorities,
                    } => {
                        state.non_retryable_stake += bad_votes;
                        if bad_votes > 0 {
                            state.errors.push((
                                SuiError::InvalidSignature {
                                    error: "Individual signature verification failed".to_string(),
                                },
                                bad_authorities,
                                bad_votes,
                            ));
                        }
                        Ok(None)
                    }
                    InsertResult::Failed { error } => Err(error),
                    InsertResult::QuorumReached(cert_sig) => {
                        let ct = CertifiedTransactionEffects::new_from_data_and_sig(
                            plain_tx_effects.into_data(),
                            cert_sig,
                        );
                        Ok(Some(ProcessTransactionResult::Executed(
                            ct.verify(&self.committee)?,
                            events,
                        )))
                    }
                }
            }
        }
    }

    /// Check if we have some signed TransactionEffects but not a quorum
    fn record_non_quorum_effects_maybe(
        &self,
        tx_digest: &TransactionDigest,
        mut state: ProcessTransactionState,
    ) -> ProcessTransactionState {
        if state.effects_map.unique_key_count() > 0 {
            let non_quorum_effects = state.effects_map.get_all_unique_values();
            warn!(
                ?tx_digest,
                "Received signed Effects but not with a quorum {:?}", non_quorum_effects
            );

            // Safe to unwrap because we know that there is at least one entry in the map
            // from the check above.
            let (_most_staked_effects_digest, (_, most_staked_effects_digest_stake)) =
                non_quorum_effects
                    .iter()
                    .max_by_key(|&(_, (_, stake))| stake)
                    .unwrap();
            // We check if we have enough retryable stake to get quorum for the most staked
            // effects digest, otherwise it indicates we have violated safety assumptions
            // or we have forked.
            if most_staked_effects_digest_stake + self.get_retryable_stake(&state)
                < self.committee.quorum_threshold()
            {
                state.retryable = false;
                if state.check_if_error_indicates_tx_finalized_with_different_user_sig(
                    self.committee.validity_threshold(),
                ) {
                    state.tx_finalized_with_different_user_sig = true;
                } else {
                    // TODO: Figure out a more reliable way to detect invariance violations.
                    error!(
                        "We have seen signed effects but unable to reach quorum threshold even including retriable stakes. This is very rare. Tx: {tx_digest:?}. Non-quorum effects: {non_quorum_effects:?}."
                    );
                }
            }

            let mut involved_validators = Vec::new();
            let mut total_stake = 0;
            for (validators, stake) in non_quorum_effects.values() {
                involved_validators.extend_from_slice(validators);
                total_stake += stake;
            }
            // TODO: Instead of pushing a new error, we should add more information about the non-quorum effects
            // in the final error if state is no longer retryable
            state.errors.push((
                SuiError::QuorumFailedToGetEffectsQuorumWhenProcessingTransaction {
                    effects_map: non_quorum_effects,
                },
                involved_validators,
                total_stake,
            ));
        }
        state
    }

    fn get_retryable_stake(&self, state: &ProcessTransactionState) -> StakeUnit {
        self.committee.total_votes()
            - state.non_retryable_stake
            - state.effects_map.total_votes()
            - state.tx_signatures.total_votes()
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn process_certificate(
        &self,
        request: HandleCertificateRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<QuorumDriverResponse, AggregatorProcessCertificateError> {
        let state = ProcessCertificateState {
            effects_map: MultiStakeAggregator::new(self.committee.clone()),
            non_retryable_stake: 0,
            non_retryable_errors: vec![],
            retryable_errors: vec![],
            retryable: true,
            events: None,
            input_objects: None,
            output_objects: None,
            auxiliary_data: None,
            request: request.clone(),
        };

        // create a set of validators that we should sample to request input/output objects from
        let validators_to_sample =
            if request.include_input_objects || request.include_output_objects {
                // Number of validators to request input/output objects from
                const NUMBER_TO_SAMPLE: usize = 10;

                self.committee
                    .choose_multiple_weighted_iter(NUMBER_TO_SAMPLE)
                    .cloned()
                    .collect()
            } else {
                HashSet::new()
            };

        let tx_digest = *request.certificate.digest();
        let timeout_after_quorum = self.timeouts.post_quorum_timeout;

        let request_ref = request;
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();

        debug!(
            ?tx_digest,
            quorum_threshold = threshold,
            validity_threshold = validity,
            ?timeout_after_quorum,
            "Broadcasting certificate to authorities"
        );
        let committee: Arc<Committee> = self.committee.clone();
        let authority_clients = self.authority_clients.clone();
        let metrics = self.metrics.clone();
        let metrics_clone = metrics.clone();
        let validator_display_names = self.validator_display_names.clone();
        let (result, mut remaining_tasks) = quorum_map_then_reduce_with_timeout(
            committee.clone(),
            authority_clients.clone(),
            state,
            move |name, client| {
                Box::pin(async move {
                    let _guard = GaugeGuard::acquire(&metrics_clone.inflight_certificate_requests);
                    let concise_name = name.concise_owned();
                    if request_ref.include_input_objects || request_ref.include_output_objects {

                        // adjust the request to validators we aren't planning on sampling
                        let req = if validators_to_sample.contains(&name) {
                            request_ref
                        } else {
                            HandleCertificateRequestV3 {
                                include_input_objects: false,
                                include_output_objects: false,
                                include_auxiliary_data: false,
                                ..request_ref
                            }
                        };

                        client
                            .handle_certificate_v3(req, client_addr)
                            .instrument(trace_span!("handle_certificate_v3", authority =? concise_name))
                            .await
                    } else {
                        client
                            .handle_certificate_v2(request_ref.certificate, client_addr)
                            .instrument(trace_span!("handle_certificate_v2", authority =? concise_name))
                            .await
                            .map(|response| HandleCertificateResponseV3 {
                                effects: response.signed_effects,
                                events: Some(response.events),
                                input_objects: None,
                                output_objects: None,
                                auxiliary_data: None,
                            })
                    }
                })
            },
            move |mut state, name, weight, response| {
                let committee_clone = committee.clone();
                let metrics = metrics.clone();
                let display_name = validator_display_names.get(&name).unwrap_or(&name.concise().to_string()).clone();
                Box::pin(async move {
                    // We aggregate the effects response, until we have more than 2f
                    // and return.
                    match AuthorityAggregator::<A>::handle_process_certificate_response(
                        committee_clone,
                        &metrics,
                        &tx_digest, &mut state, response, name)
                    {
                        Ok(Some(effects)) => ReduceOutput::Success(effects),
                        Ok(None) => {
                            // When the result is none, it is possible that the
                            // non_retryable_stake had been incremented due to
                            // failed individual signature verification.
                            if state.non_retryable_stake >= validity {
                                state.retryable = false;
                                ReduceOutput::Failed(state)
                            } else {
                                ReduceOutput::Continue(state)
                            }
                        },
                        Err(err) => {
                            let concise_name = name.concise();
                            debug!(?tx_digest, name=?concise_name, "Error processing certificate from validator: {:?}", err);
                            metrics
                                .process_cert_errors
                                .with_label_values(&[&display_name, err.as_ref()])
                                .inc();
                            Self::record_rpc_error_maybe(metrics, &display_name, &err);
                            let (retryable, categorized) = err.is_retryable();
                            if !categorized {
                                // TODO: Should minimize possible uncategorized errors here
                                // use ERROR for now to make them easier to spot.
                                error!(?tx_digest, "[WATCHOUT] uncategorized tx error: {err}");
                            }
                            if !retryable {
                                state.non_retryable_stake += weight;
                                state.non_retryable_errors.push((err, vec![name], weight));
                            } else {
                                state.retryable_errors.push((err, vec![name], weight));
                            }
                            if state.non_retryable_stake >= validity {
                                state.retryable = false;
                                ReduceOutput::Failed(state)
                            } else {
                                ReduceOutput::Continue(state)
                            }
                        }
                    }
                })
            },
            // A long timeout before we hear back from a quorum
            self.timeouts.pre_quorum_timeout,
        )
        .await
        .map_err(|state| {
            debug!(
                ?tx_digest,
                num_unique_effects = state.effects_map.unique_key_count(),
                non_retryable_stake = state.non_retryable_stake,
                "Received effects responses from validators"
            );

            // record errors and tx retryable state
            for (sui_err, _, _) in state.retryable_errors.iter().chain(state.non_retryable_errors.iter()) {
                self
                    .metrics
                    .total_aggregated_err
                    .with_label_values(&[
                        sui_err.as_ref(),
                        if state.retryable {
                            "recoverable"
                        } else {
                            "non-recoverable"
                        },
                    ])
                    .inc();
            }
            if state.retryable {
                AggregatorProcessCertificateError::RetryableExecuteCertificate {
                    retryable_errors: group_errors(state.retryable_errors),
                }
            } else {
                AggregatorProcessCertificateError::FatalExecuteCertificate {
                    non_retryable_errors: group_errors(state.non_retryable_errors),
                }
            }
        })?;

        let metrics = self.metrics.clone();
        metrics
            .remaining_tasks_when_reaching_cert_quorum
            .observe(remaining_tasks.len() as f64);
        if !remaining_tasks.is_empty() {
            // Use best efforts to send the cert to remaining validators.
            spawn_monitored_task!(async move {
                let mut timeout = Box::pin(sleep(timeout_after_quorum));
                loop {
                    tokio::select! {
                        _ = &mut timeout => {
                            debug!(?tx_digest, "Timed out in post quorum cert broadcasting: {:?}. Remaining tasks: {:?}", timeout_after_quorum, remaining_tasks.len());
                            metrics.cert_broadcasting_post_quorum_timeout.inc();
                            metrics.remaining_tasks_when_cert_broadcasting_post_quorum_timeout.observe(remaining_tasks.len() as f64);
                            break;
                        }
                        res = remaining_tasks.next() => {
                            if res.is_none() {
                                break;
                            }
                        }
                    }
                }
            });
        }
        Ok(result)
    }

    fn handle_process_certificate_response(
        committee: Arc<Committee>,
        metrics: &AuthAggMetrics,
        tx_digest: &TransactionDigest,
        state: &mut ProcessCertificateState,
        response: SuiResult<HandleCertificateResponseV3>,
        name: AuthorityName,
    ) -> SuiResult<Option<QuorumDriverResponse>> {
        match response {
            Ok(HandleCertificateResponseV3 {
                effects: signed_effects,
                events,
                input_objects,
                output_objects,
                auxiliary_data,
            }) => {
                debug!(
                    ?tx_digest,
                    name = ?name.concise(),
                    "Validator handled certificate successfully",
                );

                if events.is_some() && state.events.is_none() {
                    state.events = events;
                }

                if input_objects.is_some() && state.input_objects.is_none() {
                    state.input_objects = input_objects;
                }

                if output_objects.is_some() && state.output_objects.is_none() {
                    state.output_objects = output_objects;
                }

                if auxiliary_data.is_some() && state.auxiliary_data.is_none() {
                    state.auxiliary_data = auxiliary_data;
                }

                let effects_digest = *signed_effects.digest();
                // Note: here we aggregate votes by the hash of the effects structure
                match state.effects_map.insert(
                    (signed_effects.epoch(), effects_digest),
                    signed_effects.clone(),
                ) {
                    InsertResult::NotEnoughVotes {
                        bad_votes,
                        bad_authorities,
                    } => {
                        state.non_retryable_stake += bad_votes;
                        if bad_votes > 0 {
                            state.non_retryable_errors.push((
                                SuiError::InvalidSignature {
                                    error: "Individual signature verification failed".to_string(),
                                },
                                bad_authorities,
                                bad_votes,
                            ));
                        }
                        Ok(None)
                    }
                    InsertResult::Failed { error } => Err(error),
                    InsertResult::QuorumReached(cert_sig) => {
                        let ct = CertifiedTransactionEffects::new_from_data_and_sig(
                            signed_effects.into_data(),
                            cert_sig,
                        );

                        if (state.request.include_input_objects && state.input_objects.is_none())
                            || (state.request.include_output_objects
                                && state.output_objects.is_none())
                        {
                            metrics.quorum_reached_without_requested_objects.inc();
                            debug!(?tx_digest, "Quorum Reached but requested input/output objects were not returned");
                        }

                        ct.verify(&committee).map(|ct| {
                            debug!(?tx_digest, "Got quorum for validators handle_certificate.");
                            Some(QuorumDriverResponse {
                                effects_cert: ct,
                                events: state.events.take(),
                                input_objects: state.input_objects.take(),
                                output_objects: state.output_objects.take(),
                                auxiliary_data: state.auxiliary_data.take(),
                            })
                        })
                    }
                }
            }
            Err(err) => Err(err),
        }
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?transaction.digest()))]
    pub async fn execute_transaction_block(
        &self,
        transaction: &Transaction,
        client_addr: Option<SocketAddr>,
    ) -> Result<VerifiedCertifiedTransactionEffects, anyhow::Error> {
        let tx_guard = GaugeGuard::acquire(&self.metrics.inflight_transactions);
        let result = self
            .process_transaction(transaction.clone(), client_addr)
            .await?;
        let cert = match result {
            ProcessTransactionResult::Certified { certificate, .. } => certificate,
            ProcessTransactionResult::Executed(effects, _) => {
                return Ok(effects);
            }
        };
        self.metrics.total_tx_certificates_created.inc();
        drop(tx_guard);

        let _cert_guard = GaugeGuard::acquire(&self.metrics.inflight_certificates);
        let response = self
            .process_certificate(
                HandleCertificateRequestV3 {
                    certificate: cert.clone(),
                    include_events: true,
                    include_input_objects: false,
                    include_output_objects: false,
                    include_auxiliary_data: false,
                },
                client_addr,
            )
            .await?;

        Ok(response.effects_cert)
    }

    /// This function tries to get SignedTransaction OR CertifiedTransaction from
    /// an given list of validators who are supposed to know about it.
    #[instrument(level = "trace", skip_all, fields(?tx_digest))]
    pub async fn handle_transaction_info_request_from_some_validators(
        &self,
        tx_digest: &TransactionDigest,
        // authorities known to have the transaction info we are requesting.
        validators: &BTreeSet<AuthorityName>,
        timeout_total: Option<Duration>,
    ) -> SuiResult<PlainTransactionInfoResponse> {
        self.quorum_once_with_timeout(
            None,
            Some(validators),
            |_authority, client| {
                Box::pin(async move {
                    client
                        .handle_transaction_info_request(TransactionInfoRequest {
                            transaction_digest: *tx_digest,
                        })
                        .await
                })
            },
            Duration::from_secs(2),
            timeout_total,
            "handle_transaction_info_request_from_some_validators".to_string(),
        )
        .await
    }
}

#[derive(Default)]
pub struct AuthorityAggregatorBuilder<'a> {
    network_config: Option<&'a NetworkConfig>,
    genesis: Option<&'a Genesis>,
    committee: Option<Committee>,
    committee_store: Option<Arc<CommitteeStore>>,
    registry: Option<&'a Registry>,
    timeouts_config: Option<TimeoutConfig>,
}

impl<'a> AuthorityAggregatorBuilder<'a> {
    pub fn from_network_config(config: &'a NetworkConfig) -> Self {
        Self {
            network_config: Some(config),
            ..Default::default()
        }
    }

    pub fn from_genesis(genesis: &'a Genesis) -> Self {
        Self {
            genesis: Some(genesis),
            ..Default::default()
        }
    }

    pub fn from_committee(committee: Committee) -> Self {
        Self {
            committee: Some(committee),
            ..Default::default()
        }
    }

    pub fn with_committee_store(mut self, committee_store: Arc<CommitteeStore>) -> Self {
        self.committee_store = Some(committee_store);
        self
    }

    pub fn with_registry(mut self, registry: &'a Registry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn with_timeouts_config(mut self, timeouts_config: TimeoutConfig) -> Self {
        self.timeouts_config = Some(timeouts_config);
        self
    }

    fn get_network_committee(&self) -> CommitteeWithNetworkMetadata {
        let genesis = if let Some(network_config) = self.network_config {
            &network_config.genesis
        } else if let Some(genesis) = self.genesis {
            genesis
        } else {
            panic!("need either NetworkConfig or Genesis.");
        };
        genesis.committee_with_network()
    }

    fn get_committee(&self) -> Committee {
        self.committee
            .clone()
            .unwrap_or_else(|| self.get_network_committee().committee().clone())
    }

    pub fn build_network_clients(
        self,
    ) -> (
        AuthorityAggregator<NetworkAuthorityClient>,
        BTreeMap<AuthorityPublicKeyBytes, NetworkAuthorityClient>,
    ) {
        let network_committee = self.get_network_committee();
        let auth_clients = make_authority_clients_with_timeout_config(
            &network_committee,
            DEFAULT_CONNECT_TIMEOUT_SEC,
            DEFAULT_REQUEST_TIMEOUT_SEC,
        );
        let auth_agg = self.build_custom_clients(auth_clients.clone());
        (auth_agg, auth_clients)
    }

    pub fn build_custom_clients<C: Clone>(
        self,
        authority_clients: BTreeMap<AuthorityName, C>,
    ) -> AuthorityAggregator<C> {
        let committee = self.get_committee();
        let registry = Registry::new();
        let registry = self.registry.unwrap_or(&registry);
        let safe_client_metrics_base = SafeClientMetricsBase::new(registry);
        let auth_agg_metrics = Arc::new(AuthAggMetrics::new(registry));

        let committee_store = self
            .committee_store
            .unwrap_or_else(|| Arc::new(CommitteeStore::new_for_testing(&committee)));

        let timeouts_config = self.timeouts_config.unwrap_or_default();

        AuthorityAggregator::new(
            committee,
            committee_store,
            authority_clients,
            safe_client_metrics_base,
            auth_agg_metrics,
            Arc::new(HashMap::new()),
            timeouts_config,
        )
    }
}
