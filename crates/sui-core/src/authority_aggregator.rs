// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::{
    make_authority_clients, make_network_authority_client_sets_from_committee,
    make_network_authority_client_sets_from_system_state, AuthorityAPI, NetworkAuthorityClient,
    NetworkAuthorityClientMetrics,
};
use crate::safe_client::{SafeClient, SafeClientMetrics};
use crate::validator_info::make_committee;
use async_trait::async_trait;

use futures::{future, future::BoxFuture, stream::FuturesUnordered, StreamExt};
use itertools::Itertools;
use move_core_types::value::MoveStructLayout;
use mysten_network::config::Config;
use std::convert::AsRef;
use sui_config::genesis::Genesis;
use sui_config::NetworkConfig;
use sui_metrics::{monitored_future, spawn_monitored_task};
use sui_network::{
    default_mysten_network_config, DEFAULT_CONNECT_TIMEOUT_SEC, DEFAULT_REQUEST_TIMEOUT_SEC,
};
use sui_types::crypto::{AuthorityPublicKeyBytes, AuthoritySignInfo};
use sui_types::object::{Object, ObjectFormatOptions, ObjectRead};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{
    base_types::*,
    committee::Committee,
    error::{SuiError, SuiResult},
    messages::*,
    messages_checkpoint::{
        AuthenticatedCheckpoint, CertifiedCheckpointSummary, CheckpointContents, CheckpointRequest,
        CheckpointResponse,
    },
};
use sui_types::{fp_ensure, SUI_SYSTEM_STATE_OBJECT_ID};
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, Histogram, IntCounter, IntCounterVec, Registry,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::string::ToString;
use std::sync::Arc;
use std::time::Duration;
use sui_types::committee::{CommitteeWithNetAddresses, StakeUnit};
use tokio::sync::mpsc::Receiver;
use tokio::time::{sleep, timeout};

use crate::authority::AuthorityStore;
use crate::epoch::committee_store::CommitteeStore;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tap::TapFallible;

const OBJECT_DOWNLOAD_CHANNEL_BOUND: usize = 1024;
pub const DEFAULT_RETRIES: usize = 4;

#[cfg(test)]
#[path = "unit_tests/authority_aggregator_tests.rs"]
pub mod authority_aggregator_tests;

pub type AsyncResult<'a, T, E> = BoxFuture<'a, Result<T, E>>;

#[derive(Clone)]
pub struct TimeoutConfig {
    // Timeout used when making many concurrent requests - ok if it is large because a slow
    // authority won't block other authorities from being contacted.
    pub authority_request_timeout: Duration,
    pub pre_quorum_timeout: Duration,
    pub post_quorum_timeout: Duration,

    // Timeout used when making serial requests. Should be smaller, since we wait to hear from each
    // authority before continuing.
    pub serial_authority_request_timeout: Duration,

    // Timeout used to determine when to start a second "serial" request for
    // quorum_once_with_timeout. This is a latency optimization that prevents us from having
    // to wait an entire serial_authority_request_timeout interval before starting a second
    // request.
    //
    // If this is set to zero, then quorum_once_with_timeout becomes completely parallelized - if
    // it is set to a value greater than serial_authority_request_timeout then it becomes
    // completely serial.
    pub serial_authority_request_interval: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            authority_request_timeout: Duration::from_secs(60),
            pre_quorum_timeout: Duration::from_secs(60),
            post_quorum_timeout: Duration::from_secs(30),
            serial_authority_request_timeout: Duration::from_secs(5),
            serial_authority_request_interval: Duration::from_millis(1000),
        }
    }
}

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone)]
pub struct AuthAggMetrics {
    pub total_tx_certificates_created: IntCounter,
    pub num_signatures: Histogram,
    pub num_good_stake: Histogram,
    pub num_bad_stake: Histogram,
    pub total_quorum_once_timeout: IntCounter,
    pub process_tx_errors: IntCounterVec,
    pub process_cert_errors: IntCounterVec,
}

// Override default Prom buckets for positive numbers in 0-50k range
const POSITIVE_INT_BUCKETS: &[f64] = &[
    1., 2., 5., 10., 20., 50., 100., 200., 500., 1000., 2000., 5000., 10000., 20000., 50000.,
];

impl AuthAggMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
        Self {
            total_tx_certificates_created: register_int_counter_with_registry!(
                "total_tx_certificates_created",
                "Total number of certificates made in the authority_aggregator",
                registry,
            )
            .unwrap(),
            // It's really important to use the right histogram buckets for accurate histogram collection.
            // Otherwise values get clipped
            num_signatures: register_histogram_with_registry!(
                "num_signatures_per_tx",
                "Number of signatures collected per transaction",
                POSITIVE_INT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            num_good_stake: register_histogram_with_registry!(
                "num_good_stake_per_tx",
                "Amount of good stake collected per transaction",
                POSITIVE_INT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            num_bad_stake: register_histogram_with_registry!(
                "num_bad_stake_per_tx",
                "Amount of bad stake collected per transaction",
                POSITIVE_INT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            total_quorum_once_timeout: register_int_counter_with_registry!(
                "total_quorum_once_timeout",
                "Total number of timeout when calling quorum_once_with_timeout",
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
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = prometheus::Registry::new();
        Self::new(&registry)
    }
}

struct EffectsStakeInfo {
    stake: StakeUnit,
    effects: TransactionEffects,
    signatures: Vec<AuthoritySignInfo>,
}

#[derive(Default)]
struct EffectsStakeMap {
    effects_map: HashMap<(EpochId, TransactionEffectsDigest), EffectsStakeInfo>,
    efects_cert: Option<CertifiedTransactionEffects>,
}

impl EffectsStakeMap {
    pub fn add(
        &mut self,
        effects: SignedTransactionEffects,
        weight: StakeUnit,
        committee: &Committee,
    ) -> bool {
        let epoch = effects.epoch();
        let digest = *effects.digest();
        let (effects, sig) = effects.into_data_and_sig();
        let entry = self
            .effects_map
            .entry((epoch, digest))
            .or_insert(EffectsStakeInfo {
                stake: 0,
                effects,
                signatures: vec![],
            });
        entry.stake += weight;
        entry.signatures.push(sig);

        if entry.stake >= committee.quorum_threshold() {
            self.efects_cert = CertifiedTransactionEffects::new(
                entry.effects.clone(),
                entry.signatures.clone(),
                committee,
            )
            .tap_err(|err| {
                error!(
                    "A quorum of effects are available but failed to form a certificate: {:?}",
                    err
                );
            })
            .ok();
            self.efects_cert.is_some()
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        self.effects_map.len()
    }

    pub fn get_cert(&self) -> Option<CertifiedTransactionEffects> {
        self.efects_cert.clone()
    }
}

#[derive(Clone)]
pub struct AuthorityAggregator<A> {
    /// Our Sui committee.
    pub committee: Committee,
    /// How to talk to this committee.
    pub authority_clients: BTreeMap<AuthorityName, SafeClient<A>>,
    /// Metrics
    pub metrics: AuthAggMetrics,
    pub timeouts: TimeoutConfig,
    /// Store here for clone during re-config
    pub safe_client_metrics: Arc<SafeClientMetrics>,
    /// Store here for clone during re-config.
    /// Only relevant for NetworkAuthorityClient.
    pub network_client_metrics: Arc<NetworkAuthorityClientMetrics>,
    /// Store here for clone during re-config.
    pub committee_store: Arc<CommitteeStore>,
}

impl<A> AuthorityAggregator<A> {
    pub fn new(
        committee: Committee,
        committee_store: Arc<CommitteeStore>,
        authority_clients: BTreeMap<AuthorityName, A>,
        metrics: AuthAggMetrics,
        safe_client_metrics: Arc<SafeClientMetrics>,
        network_client_metrics: Arc<NetworkAuthorityClientMetrics>,
    ) -> Self {
        Self::new_with_timeouts(
            committee,
            committee_store,
            authority_clients,
            metrics,
            safe_client_metrics,
            network_client_metrics,
            Default::default(),
        )
    }

    pub fn new_with_timeouts(
        committee: Committee,
        committee_store: Arc<CommitteeStore>,
        authority_clients: BTreeMap<AuthorityName, A>,
        metrics: AuthAggMetrics,
        safe_client_metrics: Arc<SafeClientMetrics>,
        network_client_metrics: Arc<NetworkAuthorityClientMetrics>,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            committee,
            authority_clients: authority_clients
                .into_iter()
                .map(|(name, api)| {
                    (
                        name,
                        SafeClient::new(
                            api,
                            committee_store.clone(),
                            name,
                            safe_client_metrics.clone(),
                        ),
                    )
                })
                .collect(),
            metrics,
            timeouts,
            safe_client_metrics,
            network_client_metrics,
            committee_store,
        }
    }

    /// This function recreates AuthorityAggregator with the given committee.
    /// It also updates committee store which impacts other of its references.
    /// If it is called on a Validator/Fullnode, it **may** interleave with the the authority active's
    /// reconfiguration process, and leave the commmittee store in an inconsistent state.
    /// When catching up to the latest epoch, it should call `reconfig_from_genesis` first to fill in
    /// all previous epoch's committee info.
    pub fn recreate_with_net_addresses(
        &self,
        committee: CommitteeWithNetAddresses,
        network_config: &Config,
    ) -> SuiResult<AuthorityAggregator<NetworkAuthorityClient>> {
        let network_clients = make_network_authority_client_sets_from_committee(
            &committee,
            network_config,
            self.network_client_metrics.clone(),
        )
        .map_err(|err| SuiError::GenericAuthorityError {
            error: format!("Failed to make authority clients from committee: {:?}", err),
        })?;

        let safe_clients = network_clients
            .into_iter()
            .map(|(name, api)| {
                (
                    name,
                    SafeClient::new(
                        api,
                        self.committee_store.clone(),
                        name,
                        self.safe_client_metrics.clone(),
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();

        // TODO: It's likely safer to do the following operations atomically, in case this function
        // gets called from different threads. It cannot happen today, but worth the caution.
        let new_committee = committee.committee;
        fp_ensure!(
            self.committee.epoch + 1 == new_committee.epoch,
            SuiError::AdvanceEpochError {
                error: format!(
                    "Trying to advance from epoch {} to epoch {}",
                    self.committee.epoch, new_committee.epoch
                )
            }
        );
        // This call may return error if this committee is already inserted,
        // which is fine. We should continue to construct the new aggregator.
        // This is because there may be multiple AuthorityAggregators
        // or its containers (e.g. Quorum Drivers)  share the same committee
        // store and all of them need to reconfigure.
        let _ = self.committee_store.insert_new_committee(&new_committee);
        Ok(AuthorityAggregator {
            committee: new_committee,
            authority_clients: safe_clients,
            metrics: self.metrics.clone(),
            timeouts: self.timeouts.clone(),
            safe_client_metrics: self.safe_client_metrics.clone(),
            network_client_metrics: self.network_client_metrics.clone(),
            committee_store: self.committee_store.clone(),
        })
    }

    pub fn get_client(&self, name: &AuthorityName) -> Option<&SafeClient<A>> {
        self.authority_clients.get(name)
    }

    pub fn clone_client(&self, name: &AuthorityName) -> SafeClient<A>
    where
        A: Clone,
    {
        self.authority_clients[name].clone()
    }

    pub fn clone_inner_clients(&self) -> BTreeMap<AuthorityName, A>
    where
        A: Clone,
    {
        let mut clients = BTreeMap::new();
        for (name, client) in &self.authority_clients {
            clients.insert(*name, client.authority_client().clone());
        }
        clients
    }
}

impl AuthorityAggregator<NetworkAuthorityClient> {
    /// Create a new network authority aggregator by reading the committee and network address
    /// information from the system state object on-chain.
    pub fn new_from_system_state(
        store: &Arc<AuthorityStore>,
        committee_store: &Arc<CommitteeStore>,
        prometheus_registry: &Registry,
    ) -> anyhow::Result<Self> {
        let net_config = default_mysten_network_config();
        let sui_system_state = store.get_sui_system_state_object()?;

        let network_metrics = Arc::new(NetworkAuthorityClientMetrics::new(prometheus_registry));
        let authority_clients = make_network_authority_client_sets_from_system_state(
            &sui_system_state,
            &net_config,
            network_metrics.clone(),
        )?;
        Ok(Self::new(
            sui_system_state.get_current_epoch_committee().committee,
            committee_store.clone(),
            authority_clients,
            AuthAggMetrics::new(prometheus_registry),
            Arc::new(SafeClientMetrics::new(prometheus_registry)),
            network_metrics,
        ))
    }
}

pub enum ReduceOutput<S> {
    Continue(S),
    ContinueWithTimeout(S, Duration),
    End(S),
}

#[async_trait]
trait CertificateHandler {
    async fn handle(
        &self,
        certificate: CertifiedTransaction,
    ) -> SuiResult<VerifiedTransactionInfoResponse>;

    fn destination_name(&self) -> String;
}

// Syncs a certificate to a (possibly) remote authority.
struct RemoteCertificateHandler<A> {
    destination_authority: AuthorityName,
    destination_client: SafeClient<A>,
}

#[async_trait]
impl<A> CertificateHandler for RemoteCertificateHandler<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    async fn handle(
        &self,
        certificate: CertifiedTransaction,
    ) -> SuiResult<VerifiedTransactionInfoResponse> {
        self.destination_client
            .handle_certificate(certificate)
            .await
    }

    fn destination_name(&self) -> String {
        format!("{:?}", self.destination_authority)
    }
}

impl<A> AuthorityAggregator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    /// Sync a certificate and all its dependencies to a destination authority, using a
    /// source authority to get information about parent certificates.
    ///
    /// Note: Both source and destination may be byzantine, therefore one should always
    /// time limit the call to this function to avoid byzantine authorities consuming
    /// an unbounded amount of resources.
    #[instrument(
        name = "sync_authority_source_to_destination",
        level = "trace",
        skip_all
    )]
    async fn sync_authority_source_to_destination<CertHandler: CertificateHandler>(
        source_client: SafeClient<A>,
        cert: CertifiedTransaction,
        source_authority: AuthorityName,
        cert_handler: &CertHandler,
    ) -> Result<(), SuiError> {
        // This represents a stack of certificates that we need to register with the
        // destination authority. The stack is a LIFO queue, and therefore later insertions
        // represent certificates that earlier insertions depend on. Thus updating an
        // authority in the order we pop() the certificates from this stack should ensure
        // certificates are uploaded in causal order.
        let mut missing_certificates: Vec<_> = vec![cert.clone()];

        // We keep a list of certificates already processed to avoid duplicates
        let mut processed_certificates: HashSet<TransactionDigest> = HashSet::new();
        let mut attempted_certificates: HashSet<TransactionDigest> = HashSet::new();

        while let Some(target_cert) = missing_certificates.pop() {
            let cert_digest = *target_cert.digest();

            if processed_certificates.contains(&cert_digest) {
                continue;
            }

            debug!(tx_digest = ?cert_digest, authority =? cert_handler.destination_name(), "Running confirmation transaction for missing cert");

            match cert_handler.handle(target_cert.clone()).await {
                Ok(_) => {
                    processed_certificates.insert(cert_digest);
                    continue;
                }
                Err(SuiError::TransactionInputObjectsErrors { .. }) => {}
                Err(e) => return Err(e),
            }

            // If we are here it means that the destination authority is missing
            // the previous certificates, so we need to read them from the source
            // authority.
            debug!(
                tx_digest = ?cert_digest,
                "Missing previous certificates, need to find parents from source authorities"
            );

            // The first time we cannot find the cert from the destination authority
            // we try to get its dependencies. But the second time we have already tried
            // to update its dependencies, so we should just admit failure.
            if attempted_certificates.contains(&cert_digest) {
                trace!(tx_digest = ?cert_digest, "bailing out after second attempt to fetch");
                return Err(SuiError::AuthorityInformationUnavailable);
            }
            attempted_certificates.insert(cert_digest);

            // TODO: Eventually the client will store more information, and we could
            // first try to read certificates and parents from a local cache before
            // asking an authority.

            let transaction_info = if missing_certificates.is_empty() {
                // Here we cover a corner case due to the nature of using consistent
                // broadcast: it is possible for the client to have a certificate
                // signed by some authority, before the authority has processed the
                // certificate. This can only happen to a certificate for objects
                // not used in another certificicate, hence it can only be the case
                // for the very first certificate we try to sync. For this reason for
                // this one instead of asking for the effects of a previous execution
                // we send the cert for execution. Since execution is idempotent this
                // is ok.

                trace!(
                    ?source_authority,
                    ?cert_digest,
                    "Having source authority run confirmation again"
                );
                source_client
                    .handle_certificate(target_cert.clone())
                    .await?
            } else {
                // Unlike the previous case if a certificate created an object that
                // was involved in the processing of another certificate the previous
                // cert must have been processed, so here we just ask for the effects
                // of such an execution.

                trace!(
                    ?source_authority,
                    ?cert_digest,
                    "handle_transaction_info_request from source"
                );
                source_client
                    .handle_transaction_info_request(TransactionInfoRequest {
                        transaction_digest: cert_digest,
                    })
                    .await?
            };

            // Put back the target cert
            missing_certificates.push(target_cert);
            let signed_effects = &transaction_info
                .signed_effects
                .ok_or(SuiError::AuthorityInformationUnavailable)?;

            trace!(tx_digest = ?cert_digest, dependencies =? &signed_effects.data().dependencies, "Got dependencies from source");
            for returned_digest in &signed_effects.data().dependencies {
                trace!(tx_digest =? returned_digest, "Found parent of missing cert");

                let inner_transaction_info = source_client
                    .handle_transaction_info_request(TransactionInfoRequest {
                        transaction_digest: *returned_digest,
                    })
                    .await?;
                trace!(?returned_digest, source =? source_authority, "Got transaction info from source");

                let returned_certificate = inner_transaction_info
                    .certified_transaction
                    .ok_or(SuiError::AuthorityInformationUnavailable)?;

                // Add it to the list of certificates to sync
                trace!(?returned_digest, source =? source_authority, "Pushing transaction onto stack");
                missing_certificates.push(returned_certificate.into());
            }
        }

        Ok(())
    }

    async fn sync_certificate_to_authority(
        &self,
        cert: CertifiedTransaction,
        destination_authority: AuthorityName,
        retries: usize,
        authority_timeout: Duration,
        total_timeout: Duration,
    ) -> Result<VerifiedTransactionInfoResponse, SuiError> {
        let committee = self.committee.clone();
        let client = self
            .authority_clients
            .get(&destination_authority)
            .unwrap()
            .clone();
        let authority_clients = self.authority_clients.clone();
        if let Ok(res) = timeout(total_timeout, spawn_monitored_task!(async move {
            Self::sync_certificate_to_authority_with_timeout(
                &committee,
                &authority_clients,
                cert.clone(),
                destination_authority,
                authority_timeout,
                retries,
            )
            .await?;
            client.handle_certificate(cert).instrument(tracing::trace_span!("handle_cert_after_sync", authority =? destination_authority.concise(), retry = true)).await
        })).await {
            res.map_err(|e| SuiError::CertificateSyncError {
                authority_name: destination_authority.to_string(),
                err: e.to_string(),
            })?
        } else {
            Err(SuiError::TimeoutError)
        }
    }

    async fn sync_certificate_to_authority_with_timeout(
        committee: &Committee,
        authority_clients: &BTreeMap<AuthorityName, SafeClient<A>>,
        cert: CertifiedTransaction,
        destination_authority: AuthorityName,
        timeout_period: Duration,
        retries: usize,
    ) -> Result<(), SuiError> {
        let cert_handler = RemoteCertificateHandler {
            destination_authority,
            destination_client: authority_clients[&destination_authority].clone(),
        };
        debug!(cert =? cert.digest(),
               dest_authority =? destination_authority,
               "Syncing certificate to dest authority");
        Self::sync_certificate_to_authority_with_timeout_inner(
            committee,
            authority_clients,
            cert,
            destination_authority,
            &cert_handler,
            timeout_period,
            retries,
        )
        .await
    }

    /// Sync a certificate to an authority.
    ///
    /// This function infers which authorities have the history related to
    /// a certificate and attempts `retries` number of them, sampled according to
    /// stake, in order to bring the destination authority up to date to accept
    /// the certificate. The time devoted to each attempt is bounded by
    /// `timeout_milliseconds`.
    async fn sync_certificate_to_authority_with_timeout_inner<CertHandler: CertificateHandler>(
        committee: &Committee,
        authority_clients: &BTreeMap<AuthorityName, SafeClient<A>>,
        cert: CertifiedTransaction,
        destination_authority: AuthorityName,
        cert_handler: &CertHandler,
        timeout_period: Duration,
        retries: usize,
    ) -> Result<(), SuiError> {
        // Extract the set of authorities that should have this certificate
        // and its full history. We should be able to use these are source authorities.
        let mut candidate_source_authorties: HashSet<AuthorityName> = cert
            .auth_sig()
            .authorities(committee)
            .collect::<SuiResult<HashSet<_>>>()?
            .iter()
            .map(|&&name| name)
            .collect::<HashSet<_>>();

        // Sample a `retries` number of distinct authorities by stake.
        let mut source_authorities: Vec<AuthorityName> = Vec::new();
        while source_authorities.len() < retries && !candidate_source_authorties.is_empty() {
            // Here we do rejection sampling.
            //
            // TODO: add a filter parameter to sample, so that we can directly
            //       sample from a subset which is more efficient.
            let sample_authority = committee.sample();
            if candidate_source_authorties.contains(sample_authority) {
                candidate_source_authorties.remove(sample_authority);
                source_authorities.push(*sample_authority);
            }
        }

        // Now try to update the destination authority sequentially using
        // the source authorities we have sampled.
        for source_authority in source_authorities {
            // Note: here we could improve this function by passing into the
            //       `sync_authority_source_to_destination` call a cache of
            //       certificates and parents to avoid re-downloading them.
            let source_client = authority_clients[&source_authority].clone();
            let sync_fut = Self::sync_authority_source_to_destination(
                source_client,
                cert.clone(),
                source_authority,
                cert_handler,
            );

            // Be careful.  timeout() returning OK just means the Future completed.
            if let Ok(inner_res) = timeout(timeout_period, sync_fut).await {
                match inner_res {
                    Ok(_) => {
                        // If the updates succeeds we return, since there is no need
                        // to try other sources.
                        return Ok(());
                    }
                    // Getting here means the sync_authority_source fn finished within timeout but errored out.
                    Err(err) => {
                        // We checked that the source authority has all the information
                        // since the source has signed the certificate. Either the
                        // source or the destination authority may be faulty.

                        let inner_err = SuiError::PairwiseSyncFailed {
                            xsource: source_authority,
                            destination: destination_authority,
                            tx_digest: *cert.digest(),
                            error: Box::new(err.clone()),
                        };

                        // Report the error to both authority clients.
                        let source_client = &authority_clients[&source_authority];
                        let destination_client = &authority_clients[&destination_authority];

                        error!(
                            ?inner_err,
                            source =? source_client.address(),
                            "sync_certificate_to_authority_with_timeout_inner"
                        );
                        error!(
                            ?inner_err,
                            destination =? destination_client.address(),
                            "sync_certificate_to_authority_with_timeout_inner"
                        );

                        debug!(
                            ?source_authority,
                            ?destination_authority,
                            ?err,
                            "Error from syncing authorities, retrying"
                        );
                    }
                }
            } else {
                info!(
                    ?timeout_period,
                    "sync_authority_source_to_destination() timed out"
                );
            }

            // If we are here it means that the update failed, either due to the
            // source being faulty or the destination being faulty.
            //
            // TODO: We should probably be keeping a record of suspected faults
            // upon failure to de-prioritize authorities that we have observed being
            // less reliable.
        }

        // Eventually we should add more information to this error about the destination
        // and maybe event the certificate.
        Err(SuiError::AuthorityUpdateFailure)
    }

    /// This function takes an initial state, than executes an asynchronous function (FMap) for each
    /// authority, and folds the results as they become available into the state using an async function (FReduce).
    ///
    /// FMap can do io, and returns a result V. An error there may not be fatal, and could be consumed by the
    /// MReduce function to overall recover from it. This is necessary to ensure byzantine authorities cannot
    /// interrupt the logic of this function.
    ///
    /// FReduce returns a result to a ReduceOutput. If the result is Err the function
    /// shortcuts and the Err is returned. An Ok ReduceOutput result can be used to shortcut and return
    /// the resulting state (ReduceOutput::End), continue the folding as new states arrive (ReduceOutput::Continue),
    /// or continue with a timeout maximum waiting time (ReduceOutput::ContinueWithTimeout).
    ///
    /// This function provides a flexible way to communicate with a quorum of authorities, processing and
    /// processing their results into a safe overall result, and also safely allowing operations to continue
    /// past the quorum to ensure all authorities are up to date (up to a timeout).
    pub(crate) async fn quorum_map_then_reduce_with_timeout<'a, S, V, FMap, FReduce>(
        &'a self,
        // The initial state that will be used to fold in values from authorities.
        initial_state: S,
        // The async function used to apply to each authority. It takes an authority name,
        // and authority client parameter and returns a Result<V>.
        map_each_authority: FMap,
        // The async function that takes an accumulated state, and a new result for V from an
        // authority and returns a result to a ReduceOutput state.
        reduce_result: FReduce,
        // The initial timeout applied to all
        initial_timeout: Duration,
    ) -> Result<S, SuiError>
    where
        FMap: FnOnce(AuthorityName, &'a SafeClient<A>) -> AsyncResult<'a, V, SuiError> + Clone,
        FReduce: Fn(
            S,
            AuthorityName,
            StakeUnit,
            Result<V, SuiError>,
        ) -> AsyncResult<'a, ReduceOutput<S>, SuiError>,
    {
        self.quorum_map_then_reduce_with_timeout_and_prefs(
            None,
            initial_state,
            map_each_authority,
            reduce_result,
            initial_timeout,
        )
        .await
    }

    pub(crate) async fn quorum_map_then_reduce_with_timeout_and_prefs<'a, S, V, FMap, FReduce>(
        &'a self,
        authority_prefences: Option<&BTreeSet<AuthorityName>>,
        initial_state: S,
        map_each_authority: FMap,
        reduce_result: FReduce,
        initial_timeout: Duration,
    ) -> Result<S, SuiError>
    where
        FMap: FnOnce(AuthorityName, &'a SafeClient<A>) -> AsyncResult<'a, V, SuiError> + Clone,
        FReduce: Fn(
            S,
            AuthorityName,
            StakeUnit,
            Result<V, SuiError>,
        ) -> AsyncResult<'a, ReduceOutput<S>, SuiError>,
    {
        let authorities_shuffled = self.committee.shuffle_by_stake(authority_prefences, None);

        // First, execute in parallel for each authority FMap.
        let mut responses: futures::stream::FuturesUnordered<_> = authorities_shuffled
            .iter()
            .map(|name| {
                let client = &self.authority_clients[name];
                let execute = map_each_authority.clone();
                monitored_future!(async move {
                    (
                        *name,
                        execute(*name, client)
                            .instrument(tracing::trace_span!("quorum_map_auth", authority =? name.concise()))
                            .await,
                    )
                })
            })
            .collect();

        let mut current_timeout = initial_timeout;
        let mut accumulated_state = initial_state;
        // Then, as results become available fold them into the state using FReduce.
        while let Ok(Some((authority_name, result))) =
            timeout(current_timeout, responses.next()).await
        {
            let authority_weight = self.committee.weight(&authority_name);
            accumulated_state =
                match reduce_result(accumulated_state, authority_name, authority_weight, result)
                    .await?
                {
                    // In the first two cases we are told to continue the iteration.
                    ReduceOutput::Continue(state) => state,
                    ReduceOutput::ContinueWithTimeout(state, duration) => {
                        // Adjust the waiting timeout.
                        current_timeout = duration;
                        state
                    }
                    ReduceOutput::End(state) => {
                        // The reducer tells us that we have the result needed. Just return it.
                        return Ok(state);
                    }
                }
        }
        Ok(accumulated_state)
    }

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
        FMap: Fn(AuthorityName, SafeClient<A>) -> AsyncResult<'a, S, SuiError> + Send + Clone + 'a,
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

            let start_req = |name: AuthorityName, client: SafeClient<A>| {
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
        FMap: Fn(AuthorityName, SafeClient<A>) -> AsyncResult<'a, S, SuiError> + Send + Clone + 'a,
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
                    self.metrics.total_quorum_once_timeout.inc();
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

    /// Query validators for committee information for `epoch` (None indicates
    /// latest epoch) and try to form a CommitteeInfo if we can get a quorum.
    pub async fn get_committee_info(&self, epoch: Option<EpochId>) -> SuiResult<CommitteeInfo> {
        #[derive(Default)]
        struct GetCommitteeRequestState {
            bad_weight: StakeUnit,
            responses: BTreeMap<CommitteeInfoResponseDigest, StakeUnit>,
            errors: Vec<(AuthorityName, SuiError)>,
            committee_info: Option<CommitteeInfo>,
        }
        let initial_state = GetCommitteeRequestState::default();
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        let final_state = self
            .quorum_map_then_reduce_with_timeout(
                initial_state,
                |_name, client| {
                    Box::pin(async move {
                        client
                            .handle_committee_info_request(CommitteeInfoRequest { epoch })
                            .await
                    })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        match result {
                            Ok(resp) => {
                                let resp_digest = resp.digest();
                                if let Some(info) = resp.committee_info {
                                    let total_stake =
                                        state.responses.entry(resp_digest).or_default();
                                    *total_stake += weight;
                                    if *total_stake >= threshold {
                                        state.committee_info = Some(CommitteeInfo {
                                            epoch: resp.epoch,
                                            committee_info: info,
                                        });
                                        return Ok(ReduceOutput::End(state));
                                    }
                                } else {
                                    // This is technically unreachable because SafeClient
                                    // does the sanity check in `verify_committee_info_response`
                                    state.bad_weight += weight;
                                    state.errors.push((
                                        name,
                                        SuiError::from("Validator returns empty committee info."),
                                    ));
                                }
                            }
                            Err(err) => {
                                state.bad_weight += weight;
                                state.errors.push((name, err));
                            }
                        };

                        // Return all errors if a quorum is not possible.
                        if state.bad_weight > validity {
                            return Err(SuiError::TooManyIncorrectAuthorities {
                                errors: state.errors,
                                action: "get_committee_info".to_string(),
                            });
                        }
                        Ok(ReduceOutput::Continue(state))
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await?;

        if let Some(committee_info) = final_state.committee_info {
            Ok(committee_info)
        } else {
            Err(SuiError::TooManyIncorrectAuthorities {
                errors: final_state.errors,
                action: "get_committee_info".to_string(),
            })
        }
    }

    /// Query validators for latest SuiSystemState and try to form
    /// CommitteeWithNetworkAddress. Only return Some(CommitteeWithNetAddresses)
    /// when there is quorum. This function tolerates uninteresting
    /// differences in SuiSystemState as long as they all link to the same
    /// CommitteeWithNetAddresses.
    /// This function ignores SuiSystemState that has older epoch than
    /// `minimal_epoch`.
    /// Usually, the caller should pass in the local incumbent epoch id
    /// as `minimal_epoch`, to avoid getting confused by byzantine
    /// validators.
    pub async fn get_committee_with_net_addresses(
        &self,
        minimal_epoch: EpochId,
    ) -> SuiResult<CommitteeWithNetAddresses> {
        let (aggregate_object_info, _certificates) =
            // Skip committee check because this call usually happens when there's a potential new epoch
            self.get_object_by_id(SUI_SYSTEM_STATE_OBJECT_ID, true).await?;

        let mut committee_and_sigs = aggregate_object_info
            .into_iter()
            .filter_map(
                |(
                    (_object_ref, _transaction_digest),
                    (object_option, _layout_option, object_authorities),
                )| {
                    if let Some(object) = object_option {
                        let system_state = object.data.try_as_move().and_then(|move_object| {
                            bcs::from_bytes::<SuiSystemState>(move_object.contents()).ok()
                        })?;
                        if system_state.epoch < minimal_epoch {
                            None
                        } else {
                            let committee = system_state.get_current_epoch_committee();
                            Some((committee, object_authorities))
                        }
                    } else {
                        None
                    }
                },
            )
            .collect::<Vec<_>>();
        // Need to be sorted before applying `group_by`
        committee_and_sigs
            .sort_by(|lhs, rhs| Ord::cmp(&lhs.0.committee.epoch, &rhs.0.committee.epoch));
        let mut committee_and_votes = committee_and_sigs
            .iter()
            .group_by(|(committee, _votes)| committee.digest())
            .into_iter()
            .map(|(_committee_digest, groups)| {
                let groups = groups.collect::<Vec<_>>();
                let votes: StakeUnit = groups
                    .iter()
                    .map(|(_committee, object_authorities)| {
                        object_authorities
                            .iter()
                            .map(|(name, _)| self.committee.weight(name))
                            .sum::<StakeUnit>()
                    })
                    .sum();
                // Due to the nature of `group_by`, `groups` has at least one item
                let committee = groups[0].0.clone();
                (committee, votes)
            })
            .collect::<Vec<_>>();
        // Sort by votes. The last item is the one with the most votes, we will examine it.
        // We don't order by epoch to prevent it from being stuck when some byzantine validators
        // give wrong results. At the end of day, we need quorum to make acertain.
        committee_and_votes.sort_by(|lhs, rhs| Ord::cmp(&lhs.1, &rhs.1));
        let (committee, votes) = committee_and_votes
            .pop()
            .ok_or(SuiError::FailedToGetAgreedCommitteeFromMajority { minimal_epoch })?;
        // TODO: we could try to detect byzantine behavior here, e.g. for the same epoch
        // there are conflicting committee information.
        // If supermajority agrees on the committee state, we are good.
        if votes >= self.committee.quorum_threshold() {
            Ok(committee)
        } else {
            Err(SuiError::FailedToGetAgreedCommitteeFromMajority { minimal_epoch })
        }
    }

    /// Return all the information in the network regarding the latest state of a specific object.
    /// For each authority queried, we obtain the latest object state along with the certificate that
    /// lead up to that state. The results from each authority are aggreated for the return.
    /// The first part of the return value is a map from each unique (ObjectRef, TransactionDigest)
    /// pair to the content of the object as well as a list of authorities that responded this
    /// pair.
    /// The second part of the return value is a map from transaction digest to the cert.
    async fn get_object_by_id(
        &self,
        object_id: ObjectID,
        skip_committee_check_during_reconfig: bool,
    ) -> Result<
        (
            BTreeMap<
                (ObjectRef, TransactionDigest),
                (
                    Option<Object>,
                    Option<MoveStructLayout>,
                    Vec<(AuthorityName, Option<VerifiedSignedTransaction>)>,
                ),
            >,
            HashMap<TransactionDigest, VerifiedCertificate>,
        ),
        SuiError,
    > {
        #[derive(Default)]
        struct GetObjectByIDRequestState {
            good_weight: StakeUnit,
            bad_weight: StakeUnit,
            responses: Vec<(AuthorityName, SuiResult<VerifiedObjectInfoResponse>)>,
        }
        let initial_state = GetObjectByIDRequestState::default();
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        let final_state = self
            .quorum_map_then_reduce_with_timeout(
                initial_state,
                |_name, client| {
                    Box::pin(async move {
                        // Request and return an error if any
                        // TODO: Expose layout format option.
                        let request = ObjectInfoRequest::latest_object_info_request(
                            object_id,
                            Some(ObjectFormatOptions::default()),
                        );
                        client
                            .handle_object_info_request(
                                request,
                                skip_committee_check_during_reconfig,
                            )
                            .await
                    })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        // Here we increase the stake counter no matter if we got an error or not. The idea is that a
                        // call to ObjectInfoRequest should succeed for correct authorities no matter what. Therefore
                        // if there is an error it means that we are accessing an incorrect authority. However, an
                        // object is final if it is on 2f+1 good nodes, and any set of 2f+1 intersects with this, so
                        // after we have 2f+1 of stake (good or bad) we should get a response with the object.
                        state.good_weight += weight;
                        let is_err = result.is_err();
                        state.responses.push((name, result));

                        if is_err {
                            // We also keep an error stake counter, and if it is larger than f+1 we return an error,
                            // since either there are too many faulty authorities or we are not connected to the network.
                            state.bad_weight += weight;
                            if state.bad_weight > validity {
                                return Err(SuiError::TooManyIncorrectAuthorities {
                                    errors: state
                                        .responses
                                        .into_iter()
                                        .filter_map(|(name, response)| {
                                            response.err().map(|err| (name, err))
                                        })
                                        .collect(),
                                    action: "get_object_by_id".to_string(),
                                });
                            }
                        }

                        if state.good_weight < threshold {
                            // While we are under the threshold we wait for a longer time
                            Ok(ReduceOutput::Continue(state))
                        } else {
                            // After we reach threshold we wait for potentially less time.
                            Ok(ReduceOutput::ContinueWithTimeout(
                                state,
                                self.timeouts.post_quorum_timeout,
                            ))
                        }
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await?;

        info!("have final state");

        let mut error_list = Vec::new();
        let mut object_map = BTreeMap::<
            (ObjectRef, TransactionDigest),
            (
                Option<Object>,
                Option<MoveStructLayout>,
                Vec<(AuthorityName, Option<VerifiedSignedTransaction>)>,
            ),
        >::new();
        let mut certificates = HashMap::new();

        for (name, result) in final_state.responses {
            if let Ok(ObjectInfoResponse {
                parent_certificate,
                requested_object_reference,
                object_and_lock,
            }) = result
            {
                // Extract the object_ref and transaction digest that will be used as keys
                let object_ref = if let Some(object_ref) = requested_object_reference {
                    object_ref
                } else {
                    // The object has never been seen on this authority, so we skip
                    continue;
                };

                let (transaction_digest, cert_option) = if let Some(cert) = parent_certificate {
                    (*cert.digest(), Some(cert))
                } else {
                    (TransactionDigest::genesis(), None)
                };

                // Extract an optional object to be used in the value, note that the object can be
                // None if the object was deleted at this authority
                //
                // NOTE: here we could also be gathering the locked transactions to see if we could make a cert.
                let (object_option, signed_transaction_option, layout_option) =
                    if let Some(ObjectResponse {
                        object,
                        lock,
                        layout,
                    }) = object_and_lock
                    {
                        (Some(object), lock, layout)
                    } else {
                        (None, None, None)
                    };

                // Update the map with the information from this authority
                // TODO: if `(object_ref, transaction_digest)` is already seen, need to verify
                // the existing value matches the old value.
                let entry = object_map
                    .entry((object_ref, transaction_digest))
                    .or_insert((object_option, layout_option, Vec::new()));
                entry.2.push((name, signed_transaction_option));

                if let Some(cert) = cert_option {
                    certificates.insert(*cert.digest(), cert);
                }
            } else {
                error_list.push((name, result));
            }
        }

        // TODO: return the errors too
        Ok((object_map, certificates))
    }

    /// This function returns a map between object references owned and authorities that hold the objects
    /// at this version, as well as a list of authorities that responded to the query for the objects owned.
    ///
    /// We do not expose this function to users, as its output is hard for callers to interpret. In particular,
    /// some of the entries in the list might be the result of a query to a byzantine authority, so further
    /// sanitization and checks are necessary to rely on this information.
    ///
    /// Clients should use `sync_all_owned_objects` instead.
    async fn get_all_owned_objects(
        &self,
        address: SuiAddress,
        timeout_after_quorum: Duration,
    ) -> Result<(BTreeMap<ObjectRef, Vec<AuthorityName>>, Vec<AuthorityName>), SuiError> {
        #[derive(Default)]
        struct OwnedObjectQueryState {
            good_weight: StakeUnit,
            bad_weight: StakeUnit,
            object_map: BTreeMap<ObjectRef, Vec<AuthorityName>>,
            responded_authorities: Vec<AuthorityName>,
            errors: Vec<(AuthorityName, SuiError)>,
        }
        let initial_state = OwnedObjectQueryState::default();
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        let final_state = self
            .quorum_map_then_reduce_with_timeout(
                initial_state,
                |_name, client| {
                    // For each authority we ask all objects associated with this address, and return
                    // the result.
                    let inner_address = address;
                    Box::pin(async move {
                        client
                            .handle_account_info_request(AccountInfoRequest::from(inner_address))
                            .await
                    })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        // Here we increase the stake counter no matter if we got a correct
                        // response or not. A final transaction will have effects on 2f+1 so if we
                        // ask any 2f+1 we should get the version of the latest object.
                        state.good_weight += weight;

                        // For each non error result we get we add the objects to the map
                        // as keys and append the authority that holds them in the values.
                        match result {
                            Ok(AccountInfoResponse { object_ids, .. }) => {
                                trace!(?object_ids, name=?name.concise(), "Got response");
                                // Also keep a record of all authorities that responded.
                                state.responded_authorities.push(name);
                                // Update the map.
                                for obj_ref in object_ids {
                                    state
                                        .object_map
                                        .entry(obj_ref)
                                        .or_insert_with(Vec::new)
                                        .push(name);
                                }
                            }
                            Err(err) => {
                                state.errors.push((name, err));
                                // We also keep an error weight counter, and if it exceeds 1/3
                                // we return an error as it is likely we do not have enough
                                // evidence to return a correct result.
                                state.bad_weight += weight;
                                if state.bad_weight > validity {
                                    return Err(SuiError::TooManyIncorrectAuthorities {
                                        errors: state.errors,
                                        action: "get_all_owned_objects".to_string(),
                                    });
                                }
                            }
                        };

                        if state.good_weight < threshold {
                            // While we are under the threshold we wait for a longer time
                            Ok(ReduceOutput::Continue(state))
                        } else {
                            // After we reach threshold we wait for potentially less time.
                            Ok(ReduceOutput::ContinueWithTimeout(
                                state,
                                timeout_after_quorum,
                            ))
                        }
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await?;
        Ok((final_state.object_map, final_state.responded_authorities))
    }

    /// Takes a list of object IDs, goes to all (quorum+timeout) of authorities to find their
    /// latest version, and then updates all authorities with the latest version of each object.
    pub async fn sync_all_given_objects(
        &self,
        objects: &[ObjectID],
    ) -> Result<
        (
            Vec<(
                Object,
                Option<MoveStructLayout>,
                Option<VerifiedCertificate>,
            )>,
            Vec<(ObjectRef, Option<VerifiedCertificate>)>,
        ),
        SuiError,
    > {
        let mut active_objects = Vec::new();
        let mut deleted_objects = Vec::new();
        let mut certs_to_sync = BTreeMap::new();
        // We update each object at each authority that does not have it.
        for object_id in objects {
            // Authorities to update.
            let mut authorities: HashSet<AuthorityName> = self.committee.names().cloned().collect();

            let (aggregate_object_info, certificates) =
                self.get_object_by_id(*object_id, false).await?;

            let mut aggregate_object_info: Vec<_> = aggregate_object_info.into_iter().collect();

            // If more that one version of an object is available, we update all authorities with it.
            while !aggregate_object_info.is_empty() {
                // This will be the very latest object version, because object_ref is transactioned this way.
                let (
                    (object_ref, transaction_digest),
                    (object_option, layout_option, object_authorities),
                ) = aggregate_object_info.pop().unwrap(); // safe due to check above

                // NOTE: Here we must check that the object is indeed an input to this transaction
                //       but for the moment lets do the happy case.

                if !certificates.contains_key(&transaction_digest) {
                    // NOTE: This implies this is a genesis object. We should check that it is.
                    //       We can do this by looking into the genesis, or the object_refs of the genesis.
                    //       Otherwise report the authority as potentially faulty.

                    if let Some(obj) = object_option {
                        active_objects.push((obj, layout_option, None));
                    }
                    // Cannot be that the genesis contributes to deleted objects

                    continue;
                }

                let cert = certificates[&transaction_digest].clone(); // safe due to check above.

                // Remove authorities at this version, they will not need to be updated.
                for (name, _signed_transaction) in object_authorities {
                    authorities.remove(&name);
                }

                // NOTE: Just above we have access to signed transactions that have not quite
                //       been processed by enough authorities. We should either return them
                //       to the caller, or -- more in the spirit of this function -- do what
                //       needs to be done to force their processing if this is possible.

                // Add authorities that need to be updated
                let entry = certs_to_sync
                    .entry(*cert.digest())
                    .or_insert((cert.clone(), HashSet::new()));
                entry.1.extend(authorities);

                // Return the latest version of an object, or a deleted object
                match object_option {
                    Some(obj) => active_objects.push((obj, layout_option, Some(cert))),
                    None => deleted_objects.push((object_ref, Some(cert))),
                }

                break;
            }
        }

        for (_, (cert, authorities)) in certs_to_sync {
            for name in authorities {
                // For each certificate authority pair run a sync to update this authority to this
                // certificate.
                // NOTE: this is right now done sequentially, we should do them in parallel using
                //       the usual FuturesUnordered.
                let _result = self
                    .sync_certificate_to_authority(
                        cert.clone().into(),
                        name,
                        DEFAULT_RETRIES,
                        self.timeouts.authority_request_timeout,
                        self.timeouts.pre_quorum_timeout,
                    )
                    .await;

                // TODO: collect errors and propagate them to the right place
            }
        }

        Ok((active_objects, deleted_objects))
    }

    /// Ask authorities for the user owned objects. Then download all objects at all versions present
    /// on authorities, along with the certificates preceding them, and update lagging authorities to
    /// the latest version of the object.
    ///
    /// This function returns all objects, including those that are
    /// no more owned by the user (but were previously owned by the user), as well as a list of
    /// deleted object references.
    pub async fn sync_all_owned_objects(
        &self,
        address: SuiAddress,
        timeout_after_quorum: Duration,
    ) -> Result<
        (
            Vec<(
                Object,
                Option<MoveStructLayout>,
                Option<VerifiedCertificate>,
            )>,
            Vec<(ObjectRef, Option<VerifiedCertificate>)>,
        ),
        SuiError,
    > {
        // Contact a quorum of authorities, and return all objects they report we own.
        let (object_map, _authority_list) = self
            .get_all_owned_objects(address, timeout_after_quorum)
            .await?;

        let all_object_ids: HashSet<_> = object_map.keys().map(|object_ref| object_ref.0).collect();

        // Then sync all the owned objects
        self.sync_all_given_objects(&all_object_ids.into_iter().collect::<Vec<_>>())
            .await
    }

    /// Submits the transaction to a quorum of validators to make a certificate.
    pub async fn process_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> Result<VerifiedCertificate, SuiError> {
        // Now broadcast the transaction to all authorities.
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        let tx_digest = transaction.digest();
        debug!(
            tx_digest = ?tx_digest,
            quorum_threshold = threshold,
            validity_threshold = validity,
            "Broadcasting transaction request to authorities"
        );
        trace!("Transaction data: {:?}", transaction.data().data);

        #[derive(Default)]
        struct ProcessTransactionState {
            // The list of signatures gathered at any point
            signatures: Vec<AuthoritySignInfo>,
            // A certificate if we manage to make or find one
            certificate: Option<VerifiedCertificate>,
            effects_map: EffectsStakeMap,
            // The list of errors gathered at any point
            errors: Vec<SuiError>,
            // Tally of stake for good vs bad responses.
            good_stake: StakeUnit,
            bad_stake: StakeUnit,
            // If there are conflicting transactions, we note them down and may attempt to retry
            conflicting_tx_digests:
                BTreeMap<TransactionDigest, (Vec<(AuthorityName, ObjectRef)>, StakeUnit)>,
        }

        let state = ProcessTransactionState::default();

        let transaction_ref = &transaction;
        let state = self
            .quorum_map_then_reduce_with_timeout(
                state,
                |_name, client| {
                    Box::pin(
                        async move { client.handle_transaction(transaction_ref.clone()).await },
                    )
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        match result {
                            // If we are given back a certificate, then we do not need
                            // to re-submit this transaction, we just returned the ready made
                            // certificate. A certificate is only valid if it's formed in the
                            // current epoch.
                            Ok(VerifiedTransactionInfoResponse {
                                certified_transaction: Some(inner_certificate),
                                ..
                            }) if inner_certificate.epoch() == self.committee.epoch  => {
                                // A validator could return a certificate from an epoch that's
                                // different from what the authority aggregator is expecting.
                                // In that case, we should not accept that certificate.
                                debug!(tx_digest = ?tx_digest, name=?name.concise(), weight, "Received prev certificate from validator handle_transaction");
                                state.certificate = Some(inner_certificate);
                            }

                            // If we didn't match the above case but here, it means that we have
                            // a cert from a different epoch, and also have effects (i.e. already
                            // executed), we can accept the certificate if we get 2f+1 effects.
                            // It's an proof that the transaction has already been finalized
                            // in a different epoch, and hence it's ok to reuse the old certificate.
                            Ok(VerifiedTransactionInfoResponse {
                                signed_effects: Some(inner_effects),
                                certified_transaction: Some(inner_certificate),
                                ..
                            }) => {
                                if state.effects_map.add(inner_effects, weight, &self.committee) {
                                    debug!(
                                        tx_digest = ?tx_digest,
                                        "Got quorum for effects for certs that are from previous epochs handle_transaction"
                                    );
                                    state.certificate = Some(inner_certificate);
                                }
                            }

                            // If we get back a signed transaction, then we aggregate the
                            // new signature and check whether we have enough to form
                            // a certificate.
                            Ok(VerifiedTransactionInfoResponse {
                                signed_transaction: Some(inner_signed_transaction),
                                ..
                            }) if inner_signed_transaction.epoch() == self.committee.epoch => {
                                let tx_digest = inner_signed_transaction.digest();
                                debug!(tx_digest = ?tx_digest, name=?name.concise(), weight, "Received signed transaction from validator handle_transaction");
                                state.signatures.push(inner_signed_transaction.into_inner().into_data_and_sig().1);
                                state.good_stake += weight;
                                if state.good_stake >= threshold {
                                    self.metrics
                                        .num_signatures
                                        .observe(state.signatures.len() as f64);
                                    self.metrics.num_good_stake.observe(state.good_stake as f64);
                                    self.metrics.num_bad_stake.observe(state.bad_stake as f64);
                                    state.certificate =
                                        Some( CertifiedTransaction::new(
                                            transaction_ref.data().clone(),
                                            state.signatures.clone(),
                                            &self.committee,
                                        )?.verify(&self.committee)?);
                                }
                            }
                            // If we get back an error, then we aggregate and check
                            // if we have too many errors
                            // In this case we will not be able to use this response
                            // to make a certificate. If this happens for more than f
                            // authorities we just stop, as there is no hope to finish.
                            Err(err) => {
                                let concise_name = name.concise();
                                debug!(tx_digest = ?tx_digest, name=?concise_name, weight, "Failed to let validator sign transaction by handle_transaction: {:?}", err);
                                self.metrics.process_tx_errors.with_label_values(&[&concise_name.to_string(), err.as_ref()]).inc();

                                if let SuiError::ObjectLockConflict {
                                    obj_ref,
                                    pending_transaction,
                                } = err {
                                    let (lock_records, total_stake) = state.conflicting_tx_digests
                                        .entry(pending_transaction)
                                        .or_insert((Vec::new(), 0));
                                    lock_records.push((name, obj_ref));
                                    *total_stake += weight;
                                }

                                // Append to the list of errors
                                state.errors.push(err);
                                state.bad_stake += weight; // This is the bad stake counter
                            }
                            // In case we don't get an error but also don't get a valid value
                            Ok(ret) => {
                                // If we are here and yet there are either certs of signed tx,
                                // it's because their epoch doesn't match with the committee.
                                // This should start happen less over time as we are working on
                                // eliminating this on honest validators.
                                // Log a warning to keep track.
                                if let Some(inner_certificate) = &ret.certified_transaction {
                                    warn!(
                                        ?tx_digest,
                                        name=?name.concise(),
                                        expected_epoch=?self.committee.epoch,
                                        returned_epoch=?inner_certificate.epoch(),
                                        "Returned certificate is from wrong epoch"
                                    );
                                }
                                if let Some(inner_signed) = &ret.signed_transaction {
                                    warn!(
                                        ?tx_digest,
                                        name=?name.concise(),
                                        expected_epoch=?self.committee.epoch,
                                        returned_epoch=?inner_signed.epoch(),
                                        "Returned signed transaction is from wrong epoch"
                                    );
                                }
                                state.errors.push(
                                    SuiError::UnexpectedResultFromValidatorHandleTransaction {
                                        err: format!("{:?}", ret),
                                    },
                                );
                                state.bad_stake += weight; // This is the bad stake counter
                            }
                        };

                        if state.bad_stake > validity {
                            self.metrics
                                .num_signatures
                                .observe(state.signatures.len() as f64);
                            self.metrics.num_good_stake.observe(state.good_stake as f64);
                            self.metrics.num_bad_stake.observe(state.bad_stake as f64);
                            return Ok(ReduceOutput::End(state));
                        }

                        // If we have a certificate, then finish, otherwise continue.
                        if state.certificate.is_some() {
                            Ok(ReduceOutput::End(state))
                        } else {
                            Ok(ReduceOutput::Continue(state))
                        }
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await?;

        debug!(
            ?tx_digest,
            num_errors = state.errors.len(),
            good_stake = state.good_stake,
            bad_stake = state.bad_stake,
            num_signatures = state.signatures.len(),
            has_certificate = state.certificate.is_some(),
            "Received signatures response from validators handle_transaction"
        );
        if !state.errors.is_empty() {
            debug!(?tx_digest, "Errors received: {:?}", state.errors);
        }

        // If we have some certificate return it, or return an error.
        state
            .certificate
            .ok_or(SuiError::QuorumFailedToProcessTransaction {
                good_stake: state.good_stake,
                errors: state.errors,
                conflicting_tx_digests: state.conflicting_tx_digests,
            })
    }

    /// Process a certificate assuming that 2f+1 authorities already are up to date.
    ///
    /// This call is meant to be called after `process_transaction` returns a certificate.
    /// At that point (and after) enough authorities are up to date with all objects
    /// needed to process the certificate that a submission should succeed. However,
    /// in case an authority returns an error, we do try to bring it up to speed.
    pub async fn process_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<CertifiedTransactionEffects, SuiError> {
        #[derive(Default)]
        struct ProcessCertificateState {
            // Different authorities could return different effects.  We want at least one effect to come
            // from 2f+1 authorities, which meets quorum and can be considered the approved effect.
            // The map here allows us to count the stake for each unique effect.
            effects_map: EffectsStakeMap,
            bad_stake: StakeUnit,
            errors: Vec<SuiError>,
        }

        let state = ProcessCertificateState::default();

        let tx_digest = *certificate.digest();
        let timeout_after_quorum = self.timeouts.post_quorum_timeout;

        let cert_ref = &certificate;
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        debug!(
            tx_digest = ?tx_digest,
            quorum_threshold = threshold,
            validity_threshold = validity,
            ?timeout_after_quorum,
            "Broadcasting certificate to authorities"
        );
        let state = self
            .quorum_map_then_reduce_with_timeout(
                state,
                |name, client| {
                    Box::pin(async move {
                        // Here is the per-authority logic to process a certificate:
                        // - we try to process a cert, and return Ok on success.
                        // - we try to update the authority with the cert, and on error return Err.
                        // - we try to re-process the certificate and return the result.

                        let res =
                            client.handle_certificate(cert_ref.clone())
                                .instrument(tracing::trace_span!("handle_certificate", authority =? name.concise()))
                                .await;

                        if res.is_ok() {
                            debug!(
                                tx_digest = ?tx_digest,
                                name = ?name.concise(),
                                "Validator handled certificate successfully",
                            );
                        }

                        // The authority may have failed to process the certificate if there were
                        // missing parents. In that case, the authority will attempt to perform causal
                        // completion and execute the cert later.
                        res
                    })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        // We aggregate the effects response, until we have more than 2f
                        // and return.
                        match result {
                            Ok(VerifiedTransactionInfoResponse {
                                signed_effects: Some(inner_effects),
                                ..
                            }) => {
                                // Note: here we aggregate votes by the hash of the effects structure
                                if state.effects_map.add(inner_effects, weight, &self.committee) {
                                    debug!(
                                        tx_digest = ?tx_digest,
                                        "Got quorum for validators handle_certificate."
                                    );
                                    return Ok(ReduceOutput::End(state));
                                }
                            }
                            Err(err) => {
                                let concise_name = name.concise();
                                debug!(tx_digest = ?tx_digest, name=?name.concise(), weight, "Failed to get signed effects from validator handle_certificate: {:?}", err);
                                self.metrics.process_cert_errors.with_label_values(&[&concise_name.to_string(), err.as_ref()]).inc();
                                state.errors.push(err);
                                state.bad_stake += weight;
                                if state.bad_stake > validity {
                                    return Ok(ReduceOutput::End(state));
                                }
                            }
                            _ => { unreachable!("SafeClient should have ruled out this case") }
                        }
                        Ok(ReduceOutput::Continue(state))
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await?;

        debug!(
            tx_digest = ?tx_digest,
            num_unique_effects = state.effects_map.len(),
            bad_stake = state.bad_stake,
            "Received effects responses from validators"
        );

        // Check that one effects structure has more than 2f votes,
        // and return it.
        if let Some(cert) = state.effects_map.get_cert() {
            debug!(
                tx_digest = ?tx_digest,
                "Found an effect with good stake over threshold"
            );
            return Ok(cert);
        }

        // If none has, fail.
        Err(SuiError::QuorumFailedToExecuteCertificate {
            errors: state.errors,
        })
    }

    pub async fn execute_transaction(
        &self,
        transaction: &VerifiedTransaction,
    ) -> Result<(VerifiedCertificate, CertifiedTransactionEffects), anyhow::Error> {
        let new_certificate = self
            .process_transaction(transaction.clone())
            .instrument(tracing::debug_span!("process_tx"))
            .await?;
        self.metrics.total_tx_certificates_created.inc();
        let response = self
            .process_certificate(new_certificate.clone().into())
            .instrument(tracing::debug_span!("process_cert"))
            .await?;

        Ok((new_certificate, response))
    }

    pub async fn get_object_info_execute(&self, object_id: ObjectID) -> SuiResult<ObjectRead> {
        let (object_map, _cert_map) = self.get_object_by_id(object_id, false).await?;
        let mut object_ref_stack: Vec<_> = object_map.into_iter().collect();

        while let Some(((obj_ref, _tx_digest), (obj_option, layout_option, authorities))) =
            object_ref_stack.pop()
        {
            let stake: StakeUnit = authorities
                .iter()
                .map(|(name, _)| self.committee.weight(name))
                .sum();

            // If we have f+1 stake telling us of the latest version of the object, we just accept
            // it.
            if stake >= self.committee.validity_threshold() {
                match obj_option {
                    Some(obj) => {
                        return Ok(ObjectRead::Exists(obj_ref, obj, layout_option));
                    }
                    None => {
                        // TODO: Figure out how to find out object being wrapped instead of deleted.
                        return Ok(ObjectRead::Deleted(obj_ref));
                    }
                };
            }
        }

        Ok(ObjectRead::NotExists(object_id))
    }

    /// Given a list of object refs, download the objects.
    pub fn fetch_objects_from_authorities(
        &self,
        object_refs: BTreeSet<ObjectRef>,
    ) -> Receiver<SuiResult<Object>> {
        let (sender, receiver) = tokio::sync::mpsc::channel(OBJECT_DOWNLOAD_CHANNEL_BOUND);
        for object_ref in object_refs {
            let sender = sender.clone();
            let client = self.authority_clients.clone();
            let timeout = self.timeouts.authority_request_timeout;
            spawn_monitored_task!(Self::fetch_one_object(client, object_ref, timeout, sender,));
        }
        // Close unused channel
        drop(sender);
        receiver
    }

    /// This function fetches one object at a time, and sends back the result over the channel
    /// The object ids are also returned so the caller can determine which fetches failed
    /// NOTE: This function assumes all authorities are honest
    async fn fetch_one_object(
        authority_clients: BTreeMap<AuthorityName, SafeClient<A>>,
        object_ref: ObjectRef,
        timeout: Duration,
        sender: tokio::sync::mpsc::Sender<Result<Object, SuiError>>,
    ) {
        let object_id = object_ref.0;
        // Prepare the request
        // TODO: We should let users decide what layout they want in the result.
        let request = ObjectInfoRequest::latest_object_info_request(
            object_id,
            Some(ObjectFormatOptions::default()),
        );

        // For now assume all authorities. Assume they're all honest
        // This assumption is woeful, and should be fixed
        // TODO: https://github.com/MystenLabs/sui/issues/320
        let results = future::join_all(authority_clients.iter().map(|(_, ac)| {
            tokio::time::timeout(
                timeout,
                ac.handle_object_info_request(request.clone(), false),
            )
        }))
        .await;

        let mut ret_val: Result<Object, SuiError> = Err(SuiError::ObjectFetchFailed {
            object_id,
            err: "No authority returned the correct object".to_string(),
        });
        // Find the first non-error value
        // There are multiple reasons why we might not have an object
        // We can timeout, or the authority returns an error or simply no object
        // When we get an object back, it also might not match the digest we want
        for resp in results.into_iter().flatten().flatten() {
            match resp.object_and_lock {
                // Either the object is a shared object, in which case we don't care about its content
                // because we can never keep shared objects up-to-date.
                // Or if it's not shared object, we check if the digest matches.
                Some(o) if o.object.is_shared() || o.object.digest() == object_ref.2 => {
                    ret_val = Ok(o.object);
                    break;
                }
                _ => (),
            }
        }
        sender
            .send(ret_val)
            .await
            .expect("Cannot send object on channel after object fetch attempt");
    }

    pub async fn handle_checkpoint_request(
        &self,
        request: &CheckpointRequest,
        // authorities known to have the checkpoint we are requesting.
        authorities: &BTreeSet<AuthorityName>,
        timeout_total: Option<Duration>,
    ) -> SuiResult<CheckpointResponse> {
        self.quorum_once_with_timeout(
            None,
            Some(authorities),
            |_, client| Box::pin(async move { client.handle_checkpoint(request.clone()).await }),
            self.timeouts.serial_authority_request_timeout,
            timeout_total,
            "handle_checkpoint_request".to_string(),
        )
        .await
    }

    pub async fn get_certified_checkpoint(
        &self,
        sequence_number: CheckpointSequenceNumber,
        request_contents: bool,
        // authorities known to have the checkpoint we are requesting.
        authorities: &BTreeSet<AuthorityName>,
        timeout_total: Option<Duration>,
    ) -> SuiResult<(CertifiedCheckpointSummary, Option<CheckpointContents>)> {
        let request = CheckpointRequest::authenticated(Some(sequence_number), request_contents);
        self.quorum_once_with_timeout(
            None,
            Some(authorities),
            |_, client| {
                let r = request.clone();
                Box::pin(async move {
                    let resp = client.handle_checkpoint(r).await?;

                    if let CheckpointResponse::AuthenticatedCheckpoint {
                        checkpoint: Some(AuthenticatedCheckpoint::Certified(past)),
                        contents,
                    } = resp
                    {
                        Ok((past, contents))
                    } else {
                        Err(SuiError::GenericAuthorityError {
                            error: "expected Certified checkpoint".into(),
                        })
                    }
                })
            },
            self.timeouts.serial_authority_request_timeout,
            timeout_total,
            "get_certified_checkpoint".to_string(),
        )
        .await
    }

    /// This function tries to fetch CertifiedTransaction from any validators.
    /// Returns Error if certificate cannot be found in any validators.
    pub async fn handle_cert_info_request(
        &self,
        digest: &TransactionDigest,
        timeout_total: Option<Duration>,
    ) -> SuiResult<VerifiedTransactionInfoResponse> {
        self.quorum_once_with_timeout(
            None,
            None,
            |_authority, client| {
                Box::pin(async move {
                    let resp = client
                        .handle_transaction_info_request((*digest).into())
                        .await?;

                    if let VerifiedTransactionInfoResponse {
                        certified_transaction: Some(_),
                        signed_effects: Some(_),
                        ..
                    } = &resp
                    {
                        Ok(resp)
                    } else {
                        // TODO change this error to TransactionCertificateNotFound
                        // handle_transaction_info_request returns success even if it doesn't have
                        // any data.
                        Err(SuiError::TransactionNotFound { digest: *digest })
                    }
                })
            },
            self.timeouts.serial_authority_request_timeout,
            timeout_total,
            "handle_cert_info_request".to_string(),
        )
        .await
    }

    pub async fn handle_transaction_and_effects_info_request(
        &self,
        digests: &ExecutionDigests,
        // authorities known to have the effects we are requesting.
        authorities: Option<&BTreeSet<AuthorityName>>,
        timeout_total: Option<Duration>,
    ) -> SuiResult<(VerifiedCertificate, SignedTransactionEffects)> {
        self.quorum_once_with_timeout(
            None,
            authorities,
            |authority, client| {
                Box::pin(async move {
                    let resp = client
                        .handle_transaction_and_effects_info_request(digests)
                        .await?;

                    match (resp.certified_transaction, resp.signed_effects) {
                        (Some(cert), Some(effects)) => Ok((cert, effects)),
                        _ => {
                            if authorities.is_some() {
                                // The caller is passing in authorities that have claimed to have the
                                // cert and effects, so if they now say they don't, they're byzantine.
                                Err(SuiError::ByzantineAuthoritySuspicion {
                                    authority,
                                    reason: format!(
                                        "Validator claimed to have the cert and effects for tx {:?} but did not return them when queried",
                                        digests.transaction,
                                    )
                                })
                            } else {
                                Err(SuiError::TransactionNotFound {
                                    digest: digests.transaction,
                                })
                            }
                        }
                    }
                })
            },
            self.timeouts.serial_authority_request_timeout,
            timeout_total,
            "handle_transaction_and_effects_info_request".to_string(),
        )
        .await
    }

    /// This function tries to get SignedTransaction OR CertifiedTransaction from
    /// an given list of validators who are supposed to know about it.
    pub async fn handle_transaction_info_request_from_some_validators(
        &self,
        tx_digest: &TransactionDigest,
        // authorities known to have the transaction info we are requesting.
        validators: &BTreeSet<AuthorityName>,
        timeout_total: Option<Duration>,
    ) -> SuiResult<(
        Option<VerifiedSignedTransaction>,
        Option<VerifiedCertificate>,
    )> {
        self.quorum_once_with_timeout(
            None,
            Some(validators),
            |authority, client| {
                Box::pin(async move {
                    let response = client
                        .handle_transaction_info_request(TransactionInfoRequest {
                            transaction_digest: *tx_digest,
                        })
                        .await?;
                    if let Some(certified_transaction) = response.certified_transaction {
                        return Ok((None, Some(certified_transaction)));
                    }

                    if let Some(signed_transaction) = response.signed_transaction {
                        return Ok((Some(signed_transaction), None));
                    }

                    // This validator could not give the transaction info, but it is supposed to know about the transaction.
                    // This could also happen on epoch change boundary.
                    warn!(name=?authority.concise(), ?tx_digest, "Validator failed to give info about a transaction, it's either byzantine or just went through an epoch change");
                    Err(SuiError::ByzantineAuthoritySuspicion {
                        authority,
                        reason: format!(
                            "Validator claimed to know about tx {:?} but did not return it when queried",
                            tx_digest,
                        )
                    })
                })
            },
            Duration::from_secs(2),
            timeout_total,
            "handle_transaction_info_request_from_some_validators".to_string(),
        )
        .await
    }

    /// Given a certificate, execute the cert on remote validators (and preferentially on the
    /// signers of the cert who are guaranteed to be able to process it immediately) until we
    /// receive f+1 identical SignedTransactionEffects - at this point we know we have the
    /// true effects for the cert, because of f+1 validators, at least 1 must be honest.
    ///
    /// It is assumed that this method will not be called by any of the signers of the cert, since
    /// they can simply execute the cert locally and compute their own effects.
    pub async fn execute_cert_to_true_effects(
        &self,
        cert: &CertifiedTransaction,
    ) -> SuiResult<SignedTransactionEffects> {
        let digest = cert.digest();

        #[derive(Debug)]
        struct ExecuteCertState {
            cumulative_weight: StakeUnit,
            good_weight: StakeUnit,
            digests: HashMap<TransactionEffectsDigest, StakeUnit>,
            true_effects: Option<SignedTransactionEffects>,
            errors: Vec<(AuthorityName, SuiError)>,
        }

        let signers: BTreeSet<_> = cert
            .auth_sig()
            .authorities(&self.committee)
            .filter_map(|r| r.ok())
            .cloned()
            .collect();

        let initial_state = ExecuteCertState {
            cumulative_weight: 0,
            good_weight: 0,
            digests: HashMap::new(),
            true_effects: None,
            errors: Vec::new(),
        };

        let validity = self.committee.validity_threshold();
        let total_weight = self.committee.total_votes;

        debug!(
            ?validity,
            ?total_weight,
            ?digest,
            "execute_cert_to_true_effects"
        );
        let final_state = self
            .quorum_map_then_reduce_with_timeout_and_prefs(
                Some(&signers),
                initial_state,
                |_name, client| {
                    Box::pin(async move { client.handle_certificate(cert.clone()).await })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        state.cumulative_weight += weight;
                        match result {
                            Ok(TransactionInfoResponse {
                                signed_effects: Some(effects),
                                ..
                            }) => {
                                state.good_weight += weight;
                                trace!(name=?name.concise(), ?weight, "successfully executed cert on peer");
                                let entry = state.digests.entry(*effects.digest()).or_insert(0);
                                *entry += weight;

                                if *entry >= validity {
                                    state.true_effects = Some(effects);
                                    return Ok(ReduceOutput::End(state));
                                }
                            }
                            Err(e) => {
                                state.errors.push((name, e));
                            }
                            _ => {
                                unreachable!("SafeClient should have ruled out this case")
                            }
                        }

                        let weight_remaining = total_weight - state.cumulative_weight;
                        if weight_remaining + state.good_weight < validity {
                            // The main realistic case in which this might happen is if a validator
                            // cannot reach the rest of the committee on the network. (The
                            // unrealistic case is that the security assumption has failed).
                            info!(
                                ?digest,
                                ?total_weight,
                                ?state,
                                "cannot reach validity threshold for effects!"
                            );
                            Ok(ReduceOutput::End(state))
                        } else {
                            Ok(ReduceOutput::Continue(state))
                        }
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await?;

        final_state
            .true_effects
            .ok_or(SuiError::TooManyIncorrectAuthorities {
                errors: final_state.errors,
                action: "execute_cert_to_true_effects".to_string(),
            })
            .tap_err(|e| info!(?digest, "execute_cert_to_true_effects failed: {}", e))
    }
}

/// Given an AuthorityAggregator on genesis (epoch 0), catch up to the latest epoch and fill in
/// all past epoches' committee information.
/// Note: this function assumes >= 2/3 validators on genesis are still serving the network.
pub async fn reconfig_from_genesis(
    mut aggregator: AuthorityAggregator<NetworkAuthorityClient>,
) -> SuiResult<AuthorityAggregator<NetworkAuthorityClient>> {
    fp_ensure!(
        aggregator.committee.epoch == 0,
        SuiError::from("reconfig_from_genesis entails an authority aggregator with epoch 0")
    );
    let latest_committee = aggregator.get_committee_with_net_addresses(0).await?;
    let latest_epoch = latest_committee.committee.epoch;
    if latest_epoch == 0 {
        // If still at epoch 0, no need to reconfig
        return Ok(aggregator);
    }
    // First we fill in the committee store from 1 to latest_epoch - 1
    let mut cur_epoch = 1;
    let network_config = default_mysten_network_config();
    loop {
        if cur_epoch >= latest_epoch {
            break;
        }
        let committee = Committee::try_from(aggregator.get_committee_info(Some(cur_epoch)).await?)?;
        aggregator
            .committee_store
            .insert_new_committee(&committee)?;
        aggregator.committee = committee;
        cur_epoch += 1;
        info!(epoch = cur_epoch, "Inserted committee");
    }
    // Now transit from latest_epoch - 1 to latest_epoch
    aggregator.recreate_with_net_addresses(latest_committee, &network_config)
}

pub struct AuthorityAggregatorBuilder<'a> {
    network_config: Option<&'a NetworkConfig>,
    genesis: Option<&'a Genesis>,
    committee_store: Option<Arc<CommitteeStore>>,
    registry: Option<Arc<Registry>>,
}

impl<'a> AuthorityAggregatorBuilder<'a> {
    pub fn from_network_config(config: &'a NetworkConfig) -> Self {
        Self {
            network_config: Some(config),
            genesis: None,
            committee_store: None,
            registry: None,
        }
    }

    pub fn from_genesis(genesis: &'a Genesis) -> Self {
        Self {
            network_config: None,
            genesis: Some(genesis),
            committee_store: None,
            registry: None,
        }
    }

    pub fn with_committee_store(mut self, committee_store: Arc<CommitteeStore>) -> Self {
        self.committee_store = Some(committee_store);
        self
    }

    pub fn with_registry(mut self, registry: Arc<Registry>) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn build(
        self,
    ) -> anyhow::Result<(
        AuthorityAggregator<NetworkAuthorityClient>,
        BTreeMap<AuthorityPublicKeyBytes, NetworkAuthorityClient>,
    )> {
        let validator_info = if let Some(network_config) = self.network_config {
            network_config.validator_set()
        } else if let Some(genesis) = self.genesis {
            genesis.validator_set()
        } else {
            anyhow::bail!("need either NetworkConfig or Genesis.");
        };
        let committee = make_committee(0, validator_info)?;
        let registry = self
            .registry
            .unwrap_or_else(|| Arc::new(prometheus::Registry::new()));
        let network_metrics = Arc::new(NetworkAuthorityClientMetrics::new(&registry));

        let auth_clients = make_authority_clients(
            validator_info,
            DEFAULT_CONNECT_TIMEOUT_SEC,
            DEFAULT_REQUEST_TIMEOUT_SEC,
            network_metrics.clone(),
        );
        let committee_store = if let Some(committee_store) = self.committee_store {
            committee_store
        } else {
            Arc::new(CommitteeStore::new_for_testing(&committee))
        };
        Ok((
            AuthorityAggregator::new(
                committee,
                committee_store,
                auth_clients.clone(),
                AuthAggMetrics::new(&registry),
                Arc::new(SafeClientMetrics::new(&registry)),
                network_metrics,
            ),
            auth_clients,
        ))
    }
}
