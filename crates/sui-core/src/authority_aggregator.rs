// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::AuthorityAPI;
use crate::gateway_state::GatewayMetrics;
use crate::safe_client::SafeClient;
use async_trait::async_trait;

use futures::{future, StreamExt};
use move_core_types::value::MoveStructLayout;
use sui_types::crypto::{AuthoritySignature, PublicKeyBytes};
use sui_types::object::{Object, ObjectFormatOptions, ObjectRead};
use sui_types::{
    base_types::*,
    committee::Committee,
    error::{SuiError, SuiResult},
    messages::*,
};
use tracing::{debug, info, instrument, trace, Instrument};

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::string::ToString;
use std::time::Duration;
use sui_types::committee::StakeUnit;
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;

const OBJECT_DOWNLOAD_CHANNEL_BOUND: usize = 1024;
pub const DEFAULT_RETRIES: usize = 4;

#[cfg(test)]
#[path = "unit_tests/authority_aggregator_tests.rs"]
pub mod authority_aggregator_tests;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

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
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            authority_request_timeout: Duration::from_secs(60),
            pre_quorum_timeout: Duration::from_secs(60),
            post_quorum_timeout: Duration::from_secs(30),
            serial_authority_request_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone)]
pub struct AuthorityAggregator<A> {
    /// Our Sui committee.
    pub committee: Committee,
    /// How to talk to this committee.
    pub authority_clients: BTreeMap<AuthorityName, SafeClient<A>>,
    // Metrics
    pub metrics: GatewayMetrics,
    pub timeouts: TimeoutConfig,
}

impl<A> AuthorityAggregator<A> {
    pub fn new(
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, A>,
        metrics: GatewayMetrics,
    ) -> Self {
        Self::new_with_timeouts(committee, authority_clients, metrics, Default::default())
    }

    pub fn new_with_timeouts(
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, A>,
        metrics: GatewayMetrics,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            committee: committee.clone(),
            authority_clients: authority_clients
                .into_iter()
                .map(|(name, api)| (name, SafeClient::new(api, committee.clone(), name)))
                .collect(),
            metrics,
            timeouts,
        }
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

pub enum ReduceOutput<S> {
    Continue(S),
    ContinueWithTimeout(S, Duration),
    End(S),
}

#[async_trait]
pub trait ConfirmationTransactionHandler {
    async fn handle(&self, cert: ConfirmationTransaction) -> SuiResult<TransactionInfoResponse>;

    fn destination_name(&self) -> String;
}

// Syncs a ConfirmationTransaction to a (possibly) remote authority.
struct RemoteConfirmationTransactionHandler<A> {
    destination_authority: AuthorityName,
    destination_client: SafeClient<A>,
}

#[async_trait]
impl<A> ConfirmationTransactionHandler for RemoteConfirmationTransactionHandler<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    async fn handle(&self, cert: ConfirmationTransaction) -> SuiResult<TransactionInfoResponse> {
        self.destination_client
            .handle_confirmation_transaction(cert)
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
    pub async fn sync_authority_source_to_destination<
        CertHandler: ConfirmationTransactionHandler,
    >(
        &self,
        cert: ConfirmationTransaction,
        source_authority: AuthorityName,
        cert_handler: &CertHandler,
    ) -> Result<(), SuiError> {
        // TODO(panic): this panics
        let source_client = self.authority_clients[&source_authority].clone();

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
            let cert_digest = *target_cert.certificate.digest();

            if processed_certificates.contains(&cert_digest) {
                continue;
            }

            debug!(digest = ?cert_digest, authority =? cert_handler.destination_name(), "Running confirmation transaction for missing cert");

            match cert_handler.handle(target_cert.clone()).await {
                Ok(_) => {
                    processed_certificates.insert(cert_digest);
                    continue;
                }
                Err(SuiError::LockErrors { .. }) => {}
                Err(e) => return Err(e),
            }

            // If we are here it means that the destination authority is missing
            // the previous certificates, so we need to read them from the source
            // authority.
            debug!(
                digest = ?cert_digest,
                "Missing previous certificates, need to find parents from source authorities"
            );

            // The first time we cannot find the cert from the destination authority
            // we try to get its dependencies. But the second time we have already tried
            // to update its dependencies, so we should just admit failure.
            if attempted_certificates.contains(&cert_digest) {
                trace!(digest = ?cert_digest, "bailing out after second attempt to fetch");
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
                    .handle_confirmation_transaction(target_cert.clone())
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

            trace!(digest = ?cert_digest, dependencies =? &signed_effects.effects.dependencies, "Got dependencies from source");
            for returned_digest in &signed_effects.effects.dependencies {
                trace!(digest =? returned_digest, "Found parent of missing cert");

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
                missing_certificates.push(ConfirmationTransaction::new(returned_certificate));
            }
        }

        Ok(())
    }

    pub async fn sync_certificate_to_authority(
        &self,
        cert: ConfirmationTransaction,
        destination_authority: AuthorityName,
        retries: usize,
    ) -> Result<(), SuiError> {
        self.sync_certificate_to_authority_with_timeout(
            cert,
            destination_authority,
            self.timeouts.authority_request_timeout,
            retries,
        )
        .await
    }

    pub async fn sync_certificate_to_authority_with_timeout(
        &self,
        cert: ConfirmationTransaction,
        destination_authority: AuthorityName,
        timeout_period: Duration,
        retries: usize,
    ) -> Result<(), SuiError> {
        let cert_handler = RemoteConfirmationTransactionHandler {
            destination_authority,
            destination_client: self.authority_clients[&destination_authority].clone(),
        };
        debug!(cert =? cert.certificate.digest(),
               dest_authority =? destination_authority,
               "Syncing certificate to dest authority");
        self.sync_certificate_to_authority_with_timeout_inner(
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
    pub async fn sync_certificate_to_authority_with_timeout_inner<
        CertHandler: ConfirmationTransactionHandler,
    >(
        &self,
        cert: ConfirmationTransaction,
        destination_authority: AuthorityName,
        cert_handler: &CertHandler,
        timeout_period: Duration,
        retries: usize,
    ) -> Result<(), SuiError> {
        // Extract the set of authorities that should have this certificate
        // and its full history. We should be able to use these are source authorities.
        let mut candidate_source_authorties: HashSet<AuthorityName> = cert
            .certificate
            .auth_sign_info
            .authorities()
            .into_iter()
            .collect();

        // Sample a `retries` number of distinct authorities by stake.
        let mut source_authorities: Vec<AuthorityName> = Vec::new();
        while source_authorities.len() < retries && !candidate_source_authorties.is_empty() {
            // Here we do rejection sampling.
            //
            // TODO: add a filter parameter to sample, so that we can directly
            //       sample from a subset which is more efficient.
            let sample_authority = self.committee.sample();
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

            let sync_fut = self.sync_authority_source_to_destination(
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
                            tx_digest: *cert.certificate.digest(),
                            error: Box::new(err.clone()),
                        };

                        // Report the error to both authority clients.
                        let source_client = &self.authority_clients[&source_authority];
                        let destination_client = &self.authority_clients[&destination_authority];

                        source_client.report_client_error(inner_err.clone());
                        destination_client.report_client_error(inner_err);

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
        let authorities_shuffled = self.committee.shuffle_by_stake();

        // First, execute in parallel for each authority FMap.
        let mut responses: futures::stream::FuturesUnordered<_> = authorities_shuffled
            .map(|name| {
                let client = &self.authority_clients[name];
                let execute = map_each_authority.clone();
                async move {
                    (
                        *name,
                        execute(*name, client)
                            .instrument(tracing::trace_span!("quorum_map_auth", authority =? name))
                            .await,
                    )
                }
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

    /// Like quorum_map_then_reduce_with_timeout, but for things that need only a single
    /// successful response, such as fetching a Transaction from some authority.
    /// This is intended for cases in which byzantine authorities can time out or slow-loris, but
    /// can't give a false answer, because e.g. the digest of the response is known, or a
    /// quorum-signed object such as a checkpoint has been requested.
    pub(crate) async fn quorum_once_with_timeout<'a, S, FMap>(
        &'a self,
        // The async function used to apply to each authority. It takes an authority name,
        // and authority client parameter and returns a Result<V>.
        map_each_authority: FMap,
        timeout_each_authority: Duration,
    ) -> Result<S, SuiError>
    where
        FMap: Fn(AuthorityName, &'a SafeClient<A>) -> AsyncResult<'a, S, SuiError>,
    {
        let authorities_shuffled = self.committee.shuffle_by_stake();

        let mut authority_errors: Vec<(AuthorityName, SuiError)> = Vec::new();

        // TODO: possibly increase concurrency after first failure to reduce latency.
        for name in authorities_shuffled {
            let client = &self.authority_clients[name];

            let res = timeout(timeout_each_authority, map_each_authority(*name, client)).await;

            match res {
                // timeout
                Err(_) => authority_errors.push((*name, SuiError::TimeoutError)),
                // request completed
                Ok(inner_res) => match inner_res {
                    Err(e) => authority_errors.push((*name, e)),
                    Ok(_) => return inner_res,
                },
            }
        }

        Err(SuiError::TooManyIncorrectAuthorities {
            errors: authority_errors,
        })
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
    ) -> Result<
        (
            BTreeMap<
                (ObjectRef, TransactionDigest),
                (
                    Option<Object>,
                    Option<MoveStructLayout>,
                    Vec<(AuthorityName, Option<SignedTransaction>)>,
                ),
            >,
            HashMap<TransactionDigest, CertifiedTransaction>,
        ),
        SuiError,
    > {
        #[derive(Default)]
        struct GetObjectByIDRequestState {
            good_weight: StakeUnit,
            bad_weight: StakeUnit,
            responses: Vec<(AuthorityName, SuiResult<ObjectInfoResponse>)>,
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
                        client.handle_object_info_request(request).await
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

        let mut error_list = Vec::new();
        let mut object_map = BTreeMap::<
            (ObjectRef, TransactionDigest),
            (
                Option<Object>,
                Option<MoveStructLayout>,
                Vec<(AuthorityName, Option<SignedTransaction>)>,
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
                                trace!(?object_ids, ?name, "Got response");
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
                Option<CertifiedTransaction>,
            )>,
            Vec<(ObjectRef, Option<CertifiedTransaction>)>,
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

            let (aggregate_object_info, certificates) = self.get_object_by_id(*object_id).await?;

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
                        ConfirmationTransaction::new(cert.clone()),
                        name,
                        DEFAULT_RETRIES,
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
                Option<CertifiedTransaction>,
            )>,
            Vec<(ObjectRef, Option<CertifiedTransaction>)>,
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

    /// Takes a transaction, brings all authorities up to date with the versions of the
    /// objects needed, and then submits the transaction to make a certificate.
    pub async fn process_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<CertifiedTransaction, SuiError> {
        // Find out which objects are required by this transaction and
        // ensure they are synced on authorities.
        let required_ids: Vec<ObjectID> = transaction
            .data
            .input_objects()?
            .iter()
            .map(|o| o.object_id())
            .collect();

        let (_active_objects, _deleted_objects) =
            self.sync_all_given_objects(&required_ids).await?;

        // Now broadcast the transaction to all authorities.
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        debug!(
            quorum_threshold = threshold,
            validity_threshold = validity,
            "Broadcasting transaction request to authorities"
        );
        trace!("Transaction data: {:?}", transaction.data);

        struct ProcessTransactionState {
            // The list of signatures gathered at any point
            signatures: Vec<(AuthorityName, AuthoritySignature)>,
            // A certificate if we manage to make or find one
            certificate: Option<CertifiedTransaction>,
            // The list of errors gathered at any point
            errors: Vec<SuiError>,
            // Tally of stake for good vs bad responses.
            good_stake: StakeUnit,
            bad_stake: StakeUnit,
        }

        let state = ProcessTransactionState {
            signatures: vec![],
            certificate: None,
            errors: vec![],
            good_stake: 0,
            bad_stake: 0,
        };

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
                            // certificate.
                            Ok(TransactionInfoResponse {
                                certified_transaction: Some(inner_certificate),
                                ..
                            }) => {
                                trace!(?name, weight, "Received prev certificate from authority");
                                state.certificate = Some(inner_certificate);
                            }

                            // If we get back a signed transaction, then we aggregate the
                            // new signature and check whether we have enough to form
                            // a certificate.
                            Ok(TransactionInfoResponse {
                                signed_transaction: Some(inner_signed_transaction),
                                ..
                            }) => {
                                state.signatures.push((
                                    name,
                                    inner_signed_transaction.auth_sign_info.signature,
                                ));
                                state.good_stake += weight;
                                if state.good_stake >= threshold {
                                    self.metrics
                                        .num_signatures
                                        .observe(state.signatures.len() as f64);
                                    self.metrics.num_good_stake.observe(state.good_stake as f64);
                                    self.metrics.num_bad_stake.observe(state.bad_stake as f64);
                                    state.certificate =
                                        Some(CertifiedTransaction::new_with_signatures(
                                            self.committee.epoch(),
                                            transaction_ref.clone(),
                                            state.signatures.clone(),
                                        )?);
                                }
                            }
                            // If we get back an error, then we aggregate and check
                            // if we have too many errors
                            // In this case we will not be able to use this response
                            // to make a certificate. If this happens for more than f
                            // authorities we just stop, as there is no hope to finish.
                            Err(err) => {
                                // We have an error here.
                                // Append to the list off errors
                                state.errors.push(err);
                                state.bad_stake += weight; // This is the bad stake counter
                            }
                            // In case we don't get an error but also don't get a valid value
                            ret => {
                                state.errors.push(
                                    SuiError::ErrorWhileProcessingTransactionTransaction {
                                        err: format!("Unexpected: {:?}", ret),
                                    },
                                );
                                state.bad_stake += weight; // This is the bad stake counter
                            }
                        };

                        if state.bad_stake > validity {
                            // Too many errors
                            debug!(
                                num_errors = state.errors.len(),
                                bad_stake = state.bad_stake,
                                "Too many errors, validity threshold exceeded. Errors={:?}",
                                state.errors
                            );
                            self.metrics
                                .num_signatures
                                .observe(state.signatures.len() as f64);
                            self.metrics.num_good_stake.observe(state.good_stake as f64);
                            self.metrics.num_bad_stake.observe(state.bad_stake as f64);

                            let unique_errors: HashSet<_> = state.errors.into_iter().collect();
                            // If no authority succeeded and all authorities returned the same error,
                            // return that error.
                            if unique_errors.len() == 1 && state.good_stake == 0 {
                                return Err(unique_errors.into_iter().next().unwrap());
                            } else {
                                return Err(SuiError::QuorumNotReached {
                                    errors: unique_errors.into_iter().collect(),
                                });
                            }
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
            num_errors = state.errors.len(),
            good_stake = state.good_stake,
            bad_stake = state.bad_stake,
            num_signatures = state.signatures.len(),
            has_certificate = state.certificate.is_some(),
            "Received signatures response from authorities for transaction req broadcast"
        );
        if !state.errors.is_empty() {
            trace!("Errors received: {:?}", state.errors);
        }

        // If we have some certificate return it, or return an error.
        state
            .certificate
            .ok_or_else(|| SuiError::ErrorWhileProcessingTransactionTransaction {
                err: format!("No certificate: {:?}", state.errors),
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
        struct EffectsStakeInfo {
            stake: StakeUnit,
            effects: TransactionEffects,
            signatures: Vec<(AuthorityName, AuthoritySignature)>,
        }
        struct ProcessCertificateState {
            // Different authorities could return different effects.  We want at least one effect to come
            // from 2f+1 authorities, which meets quorum and can be considered the approved effect.
            // The map here allows us to count the stake for each unique effect.
            effects_map: HashMap<[u8; 32], EffectsStakeInfo>,
            bad_stake: StakeUnit,
            errors: Vec<SuiError>,
        }

        let state = ProcessCertificateState {
            effects_map: HashMap::new(),
            bad_stake: 0,
            errors: vec![],
        };

        let timeout_after_quorum = self.timeouts.post_quorum_timeout;

        let cert_ref = &certificate;
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        debug!(
            quorum_threshold = threshold,
            validity_threshold = validity,
            ?timeout_after_quorum,
            "Broadcasting certificate to authorities"
        );
        let contains_shared_object = certificate.contains_shared_object();

        let state = self
            .quorum_map_then_reduce_with_timeout(
                state,
                |name, client| {
                    Box::pin(async move {
                        // Here is the per-authority logic to process a certificate:
                        // - we try to process a cert, and return Ok on success.
                        // - we try to update the authority with the cert, and on error return Err.
                        // - we try to re-process the certificate and return the result.

                        let res = if contains_shared_object {
                            client.handle_consensus_transaction(ConsensusTransaction::UserTransaction(Box::new(cert_ref.clone())))
                                .instrument(tracing::trace_span!("handle_consensus_cert", authority =? name))
                                .await
                        } else {
                            client
                            .handle_confirmation_transaction(ConfirmationTransaction::new(
                                cert_ref.clone(),
                            ))
                                .instrument(tracing::trace_span!("handle_cert", authority =? name))
                                .await
                        };

                        if res.is_ok() {
                            // We got an ok answer, so returning the result of processing
                            // the transaction.
                            return res;
                        }

                        // LockErrors indicate the authority may be out-of-date.
                        // We only attempt to update authority and retry if we are seeing LockErrors.
                        // For any other error, we stop here and return.
                        if !matches!(res, Err(SuiError::LockErrors { .. })) {
                            debug!("Error from handle_confirmation_transaction(): {:?}", res);
                            return res;
                        }

                        debug!(authority =? name, error =? res, ?timeout_after_quorum, "Authority out of date - syncing certificates");
                        // If we got LockErrors, we try to update the authority.
                        self
                            .sync_certificate_to_authority(
                                ConfirmationTransaction::new(cert_ref.clone()),
                                name,
                                DEFAULT_RETRIES,
                            )
                            .instrument(tracing::trace_span!("sync_cert", authority =? name))
                            .await
                            .map_err(|e| { info!(err =? e, "Error from sync_certificate"); e})?;

                        // Now try again
                        client
                            .handle_confirmation_transaction(ConfirmationTransaction::new(
                                cert_ref.clone(),
                            ))
                            .instrument(tracing::trace_span!("handle_cert_after_sync", authority =? name, retry = true))
                            .await
                    })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        // We aggregate the effects response, until we have more than 2f
                        // and return.
                        match result {
                            Ok(TransactionInfoResponse {
                                signed_effects: Some(inner_effects),
                                ..
                            }) => {
                                // Note: here we aggregate votes by the hash of the effects structure
                                let entry = state
                                    .effects_map
                                    .entry(inner_effects.digest())
                                    .or_insert(EffectsStakeInfo {
                                        stake: 0,
                                        effects: inner_effects.effects,
                                        signatures: vec![],
                                    });
                                entry.stake += weight;
                                entry.signatures.push((name, inner_effects.auth_signature.signature));

                                if entry.stake >= threshold {
                                    // It will set the timeout quite high.
                                    return Ok(ReduceOutput::ContinueWithTimeout(
                                        state,
                                        timeout_after_quorum,
                                    ));
                                }
                            }
                            maybe_err => {
                                // Returning Ok but without signed effects is unexpected.
                                let err = match maybe_err {
                                    Err(err) => err,
                                    Ok(_) => SuiError::ByzantineAuthoritySuspicion {
                                        authority: name,
                                    }
                                };
                                state.errors.push(err);
                                state.bad_stake += weight;
                                if state.bad_stake > validity {
                                    debug!(bad_stake = state.bad_stake,
                                        "Too many bad responses from cert processing, validity threshold exceeded.");
                                    return Err(SuiError::QuorumFailedToExecuteCertificate { errors: state.errors });
                                }
                            }
                        }
                        Ok(ReduceOutput::Continue(state))
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await?;

        debug!(
            num_unique_effects = state.effects_map.len(),
            bad_stake = state.bad_stake,
            "Received effects responses from authorities"
        );

        // Check that one effects structure has more than 2f votes,
        // and return it.
        for stake_info in state.effects_map.into_values() {
            let EffectsStakeInfo {
                stake,
                effects,
                signatures,
            } = stake_info;
            if stake >= threshold {
                debug!(
                    good_stake = stake,
                    "Found an effect with good stake over threshold"
                );
                return Ok(CertifiedTransactionEffects::new(
                    certificate.auth_sign_info.epoch,
                    effects,
                    signatures,
                )?);
            }
        }

        // If none has, fail.
        Err(SuiError::QuorumFailedToExecuteCertificate {
            errors: state.errors,
        })
    }

    /// Find the higgest sequence number that is known to a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_latest_sequence_number(&self, object_id: ObjectID) -> SequenceNumber {
        let (object_infos, _certificates) = self.get_object_by_id(object_id).await.unwrap(); // Not safe, but want to blow up if testing.
        let top_ref = object_infos.keys().last().unwrap().0;
        top_ref.1
    }

    pub async fn execute_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<(CertifiedTransaction, CertifiedTransactionEffects), anyhow::Error> {
        let new_certificate = self
            .process_transaction(transaction.clone())
            .instrument(tracing::debug_span!("process_tx"))
            .await?;
        self.metrics.total_tx_certificates.inc();
        let response = self
            .process_certificate(new_certificate.clone())
            .instrument(tracing::debug_span!("process_cert"))
            .await?;

        Ok((new_certificate, response))
    }

    pub async fn get_object_info_execute(&self, object_id: ObjectID) -> SuiResult<ObjectRead> {
        let (object_map, cert_map) = self.get_object_by_id(object_id).await?;
        let mut object_ref_stack: Vec<_> = object_map.into_iter().collect();

        while let Some(((obj_ref, tx_digest), (obj_option, layout_option, authorities))) =
            object_ref_stack.pop()
        {
            let stake: StakeUnit = authorities
                .iter()
                .map(|(name, _)| self.committee.weight(name))
                .sum();

            let mut is_ok = false;
            if stake >= self.committee.validity_threshold() {
                // If we have f+1 stake telling us of the latest version of the object, we just accept it.
                is_ok = true;
            } else if cert_map.contains_key(&tx_digest) {
                // If we have less stake telling us about the latest state of an object
                // we re-run the certificate on all authorities to ensure it is correct.
                if let Ok(effects) = self.process_certificate(cert_map[&tx_digest].clone()).await {
                    if effects.effects.is_object_mutated_here(obj_ref) {
                        is_ok = true;
                    } else {
                        // TODO: Report a byzantine fault here
                        continue;
                    }
                }
            }
            if is_ok {
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
            tokio::spawn(Self::fetch_one_object(
                self.authority_clients.clone(),
                object_ref,
                self.timeouts.authority_request_timeout,
                sender,
            ));
        }
        // Close unused channel
        drop(sender);
        receiver
    }

    /// This function fetches one object at a time, and sends back the result over the channel
    /// The object ids are also returned so the caller can determine which fetches failed
    /// NOTE: This function assumes all authorities are honest
    async fn fetch_one_object(
        authority_clients: BTreeMap<PublicKeyBytes, SafeClient<A>>,
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
            tokio::time::timeout(timeout, ac.handle_object_info_request(request.clone()))
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

    pub async fn handle_transaction_and_effects_info_request(
        &self,
        digests: &ExecutionDigests,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.quorum_once_with_timeout(
            |_name, client| {
                Box::pin(async move {
                    client
                        .handle_transaction_and_effects_info_request(digests)
                        .await
                })
            },
            // A long timeout before we hear back from a quorum
            self.timeouts.serial_authority_request_timeout,
        )
        .await
    }
}
