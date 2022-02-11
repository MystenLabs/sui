// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::AuthorityAPI;
use crate::safe_client::SafeClient;

use fastx_types::object::{Object, ObjectRead};
use fastx_types::{
    base_types::*,
    committee::Committee,
    error::{SuiError, SuiResult},
    messages::*,
};
use futures::{future, StreamExt};

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;

// TODO: Make timeout duration configurable.
const AUTHORITY_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const OBJECT_DOWNLOAD_CHANNEL_BOUND: usize = 1024;
pub const DEFAULT_RETRIES: usize = 4;

#[cfg(test)]
#[path = "unit_tests/client_tests.rs"]
mod client_tests;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub struct AuthorityAggregator<AuthorityAPI> {
    /// Our Sui committee.
    pub committee: Committee,
    /// How to talk to this committee.
    authority_clients: BTreeMap<AuthorityName, SafeClient<AuthorityAPI>>,
}

impl<AuthorityAPI> AuthorityAggregator<AuthorityAPI> {
    pub fn new(
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, AuthorityAPI>,
    ) -> Self {
        Self {
            committee: committee.clone(),
            authority_clients: authority_clients
                .into_iter()
                .map(|(name, api)| (name, SafeClient::new(api, committee.clone(), name)))
                .collect(),
        }
    }
}

pub enum ReduceOutput<S> {
    Continue(S),
    ContinueWithTimeout(S, Duration),
    End(S),
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
    async fn sync_authority_source_to_destination(
        &self,
        cert: ConfirmationOrder,
        source_authority: AuthorityName,
        destination_authority: AuthorityName,
    ) -> Result<(), SuiError> {
        let source_client = self.authority_clients[&source_authority].clone();
        let destination_client = self.authority_clients[&destination_authority].clone();

        // This represents a stack of certificates that we need to register with the
        // destination authority. The stack is a LIFO queue, and therefore later insertions
        // represent certificates that earlier insertions depend on. Thus updating an
        // authority in the order we pop() the certificates from this stack should ensure
        // certificates are uploaded in causal order.
        let digest = cert.certificate.order.digest();
        let mut missing_certificates: Vec<_> = vec![cert.clone()];

        // We keep a list of certificates already processed to avoid duplicates
        let mut candidate_certificates: HashSet<TransactionDigest> =
            vec![digest].into_iter().collect();
        let mut attempted_certificates: HashSet<TransactionDigest> = HashSet::new();

        while let Some(target_cert) = missing_certificates.pop() {
            match destination_client
                .handle_confirmation_order(target_cert.clone())
                .await
            {
                Ok(_) => continue,
                Err(SuiError::LockErrors { .. }) => {}
                Err(e) => return Err(e),
            }

            // If we are here it means that the destination authority is missing
            // the previous certificates, so we need to read them from the source
            // authority.

            // The first time we cannot find the cert from the destination authority
            // we try to get its dependencies. But the second time we have already tried
            // to update its dependencies, so we should just admit failure.
            let cert_digest = target_cert.certificate.order.digest();
            if attempted_certificates.contains(&cert_digest) {
                return Err(SuiError::AuthorityInformationUnavailable);
            }
            attempted_certificates.insert(cert_digest);

            // TODO: Eventually the client will store more information, and we could
            // first try to read certificates and parents from a local cache before
            // asking an authority.

            let order_info = if missing_certificates.is_empty() {
                // Here we cover a corner case due to the nature of using consistent
                // broadcast: it is possible for the client to have a certificate
                // signed by some authority, before the authority has processed the
                // certificate. This can only happen to a certificate for objects
                // not used in another certificicate, hence it can only be the case
                // for the very first certificate we try to sync. For this reason for
                // this one instead of asking for the effects of a previous execution
                // we send the cert for execution. Since execution is idempotent this
                // is ok.

                source_client
                    .handle_confirmation_order(target_cert.clone())
                    .await?
            } else {
                // Unlike the previous case if a certificate created an object that
                // was involved in the processing of another certificate the previous
                // cert must have been processed, so here we just ask for the effects
                // of such an execution.

                source_client
                    .handle_order_info_request(OrderInfoRequest {
                        transaction_digest: cert_digest,
                    })
                    .await?
            };

            // Put back the target cert
            missing_certificates.push(target_cert);
            let signed_effects = &order_info
                .signed_effects
                .ok_or(SuiError::AuthorityInformationUnavailable)?;

            for returned_digest in &signed_effects.effects.dependencies {
                // We check that we are not processing twice the same certificate, as
                // it would be common if two objects used by one order, were also both
                // mutated by the same preceeding order.
                if !candidate_certificates.contains(returned_digest) {
                    // Add this cert to the set we have processed
                    candidate_certificates.insert(*returned_digest);

                    let inner_order_info = source_client
                        .handle_order_info_request(OrderInfoRequest {
                            transaction_digest: *returned_digest,
                        })
                        .await?;

                    let returned_certificate = inner_order_info
                        .certified_order
                        .ok_or(SuiError::AuthorityInformationUnavailable)?;

                    // Check & Add it to the list of certificates to sync
                    returned_certificate.check(&self.committee).map_err(|_| {
                        SuiError::ByzantineAuthoritySuspicion {
                            authority: source_authority,
                        }
                    })?;
                    missing_certificates.push(ConfirmationOrder::new(returned_certificate));
                }
            }
        }

        Ok(())
    }

    /// Sync a certificate to an authority.
    ///
    /// This function infers which authorities have the history related to
    /// a certificate and attempts `retries` number of them, sampled accoding to
    /// stake, in order to bring the destination authority up to date to accept
    /// the certificate. The time devoted to each attempt is bounded by
    /// `timeout_milliseconds`.
    async fn sync_certificate_to_authority_with_timeout(
        &self,
        cert: ConfirmationOrder,
        destination_authority: AuthorityName,
        timeout_period: Duration,
        retries: usize,
    ) -> Result<(), SuiError> {
        // Extract the set of authorities that should have this certificate
        // and its full history. We should be able to use these are source authorities.
        let mut candidate_source_authorties: HashSet<AuthorityName> = cert
            .certificate
            .signatures
            .iter()
            .map(|(name, _)| *name)
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
            if timeout(
                timeout_period,
                self.sync_authority_source_to_destination(
                    cert.clone(),
                    source_authority,
                    destination_authority,
                ),
            )
            .await
            .is_ok()
            {
                // If the updates suceeds we return, since there is no need
                // to try other sources.
                return Ok(());
            }

            // If we are here it means that the update failed, either due to the
            // source being faulty or the destination being faulty.
            //
            // TODO: We should probably be keeping a record of suspected faults
            // upon failure to de-prioritize authorities that we have observed being
            // less reliable.
        }

        // Eventually we should add more information to this error about the destination
        // and maybe event the certificiate.
        Err(SuiError::AuthorityUpdateFailure)
    }

    /// This function takes an initial state, than executes an asynchronous function (FMap) for each
    /// uthority, and folds the results as they become available into the state using an async function (FReduce).
    ///
    /// FMap can do io, and returns a result V. An error there may not be fatal, and could be consumed by the
    /// MReduce function to overall recover from it. This is necessary to ensure byzantine authorities cannot
    /// interupt the logic of this function.
    ///
    /// FReduce returns a result to a ReduceOutput. If the result is Err the function
    /// shortcuts and the Err is returned. An Ok ReduceOutput result can be used to shortcut and return
    /// the resulting state (ReduceOutput::End), continue the folding as new states arrive (ReduceOutput::Continue),
    /// or continue with a timeout maximum waiting time (ReduceOutput::ContinueWithTimeout).
    ///
    /// This function provides a flexible way to communicate with a quorum of authorities, processing and
    /// processing their results into a safe overall result, and also safely allowing operations to continue
    /// past the quorum to ensure all authorities are up to date (up to a timeout).
    async fn quorum_map_then_reduce_with_timeout<'a, S, V, FMap, FReduce>(
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
            usize,
            Result<V, SuiError>,
        ) -> AsyncResult<'a, ReduceOutput<S>, SuiError>,
    {
        // TODO: shuffle here according to stake
        let authority_clients = &self.authority_clients;

        // First, execute in parallel for each authority FMap.
        let mut responses: futures::stream::FuturesUnordered<_> = authority_clients
            .iter()
            .map(|(name, client)| {
                let execute = map_each_authority.clone();
                async move { (*name, execute(*name, client).await) }
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

    /// Return all the information in the network about a specific object, including all versions of it
    /// as well as all certificates that lead to the versions and the authorities at that version.
    async fn get_object_by_id(
        &self,
        object_id: ObjectID,
        timeout_after_quorum: Duration,
    ) -> Result<
        (
            BTreeMap<
                (ObjectRef, TransactionDigest),
                (Option<Object>, Vec<(AuthorityName, Option<SignedOrder>)>),
            >,
            HashMap<TransactionDigest, CertifiedOrder>,
        ),
        SuiError,
    > {
        let initial_state = ((0usize, 0usize), Vec::new());
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        let final_state = self
            .quorum_map_then_reduce_with_timeout(
                initial_state,
                |_name, client| {
                    Box::pin(async move {
                        // Request and return an error if any
                        let request = ObjectInfoRequest::from(object_id);
                        client.handle_object_info_request(request).await
                    })
                },
                |(mut total_stake, mut state), name, weight, result| {
                    Box::pin(async move {
                        // Here we increase the stake counter no matter if we got an error or not. The idea is that a
                        // call to ObjectInfoRequest should suceed for correct authorities no matter what. Therefore
                        // if there is an error it means that we are accessing an incorrect authority. However, an
                        // object is final if it is on 2f+1 good nodes, and any set of 2f+1 intersects with this, so
                        // after we have 2f+1 of stake (good or bad) we should get a response with the object.
                        total_stake.0 += weight;

                        if result.is_err() {
                            // We also keep an error stake counter, and if it is larger than f+1 we return an error,
                            // since either there are too many faulty authorities or we are not connected to the network.
                            total_stake.1 += weight;
                            if total_stake.1 > validity {
                                return Err(SuiError::TooManyIncorrectAuthorities);
                            }
                        }

                        state.push((name, result));

                        if total_stake.0 < threshold {
                            // While we are under the threshold we wait for a longer time
                            Ok(ReduceOutput::Continue((total_stake, state)))
                        } else {
                            // After we reach threshold we wait for potentially less time.
                            Ok(ReduceOutput::ContinueWithTimeout(
                                (total_stake, state),
                                timeout_after_quorum,
                            ))
                        }
                    })
                },
                // A long timeout before we hear back from a quorum
                Duration::from_secs(60),
            )
            .await?;

        let mut error_list = Vec::new();
        let mut object_map = BTreeMap::<
            (ObjectRef, TransactionDigest),
            (Option<Object>, Vec<(AuthorityName, Option<SignedOrder>)>),
        >::new();
        let mut certificates = HashMap::new();

        for (name, result) in final_state.1 {
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
                    (cert.order.digest(), Some(cert))
                } else {
                    (TransactionDigest::genesis(), None)
                };

                // Extract an optional object to be used in the value, note that the object can be
                // None if the object was deleted at this authority
                //
                // NOTE: here we could also be gathering the locked orders to see if we could make a cert.
                let (object_option, signed_order_option) =
                    if let Some(ObjectResponse { object, lock }) = object_and_lock {
                        (Some(object), lock)
                    } else {
                        (None, None)
                    };

                // Update the map with the information from this authority
                let entry = object_map
                    .entry((object_ref, transaction_digest))
                    .or_insert((object_option, Vec::new()));
                entry.1.push((name, signed_order_option));

                if let Some(cert) = cert_option {
                    certificates.insert(cert.order.digest(), cert);
                }
            } else {
                error_list.push((name, result));
            }
        }

        // TODO: return the errors too
        Ok((object_map, certificates))
    }

    /// This function returns a map between object references owned and authorities that hold the objects
    /// at this version, as well as a list of authorities that responsed to the query for the objects owned.
    ///
    /// We do not expose this function to users, as its output is hard for callers to interpet. In particular,
    /// some of the entries in the list might be the result of a query to a byzantine authority, so further
    /// sanitization and checks are necessary to rely on this information.
    ///
    /// Clients should use `sync_all_owned_objects` instead.
    async fn get_all_owned_objects(
        &self,
        address: SuiAddress,
        timeout_after_quorum: Duration,
    ) -> Result<(BTreeMap<ObjectRef, Vec<AuthorityName>>, Vec<AuthorityName>), SuiError> {
        let initial_state = (
            (0usize, 0usize),
            BTreeMap::<ObjectRef, Vec<AuthorityName>>::new(),
            Vec::new(),
        );
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        let (_, object_map, authority_list) = self
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
                |mut state, name, weight, _result| {
                    Box::pin(async move {
                        // Here we increase the stake counter no matter if we got a correct
                        // response or not. A final order will have effects on 2f+1 so if we
                        // ask any 2f+1 we should get the version of the latest object.
                        state.0 .0 += weight;

                        // For each non error result we get we add the objects to the map
                        // as keys and append the authority that holds them in the values.
                        if let Ok(AccountInfoResponse { object_ids, .. }) = _result {
                            // Also keep a record of all authorities that responded.
                            state.2.push(name);
                            // Update the map.
                            for obj_ref in object_ids {
                                state.1.entry(obj_ref).or_insert_with(Vec::new).push(name);
                            }
                        } else {
                            // We also keep an error weight counter, and if it exceeds 1/3
                            // we return an error as it is likely we do not have enough
                            // evidence to return a correct result.

                            state.0 .1 += weight;
                            if state.0 .1 > validity {
                                return Err(SuiError::TooManyIncorrectAuthorities);
                            }
                        }

                        if state.0 .0 < threshold {
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
                Duration::from_secs(60),
            )
            .await?;
        Ok((object_map, authority_list))
    }

    /// Takes a list of object IDs, goes to all (quorum+timeout) of authorities to find their
    /// latest version, and then updates all authorities with the latest version of each object.
    pub async fn sync_all_given_objects(
        &self,
        objects: &[ObjectID],
        timeout_after_quorum: Duration,
    ) -> Result<
        (
            Vec<(Object, Option<CertifiedOrder>)>,
            Vec<(ObjectRef, Option<CertifiedOrder>)>,
        ),
        SuiError,
    > {
        let mut active_objects = Vec::new();
        let mut deleted_objects = Vec::new();
        let mut certs_to_sync = BTreeMap::new();
        // We update each object at each authority that does not have it.
        for object_id in objects {
            // Authorities to update.
            let mut authorites: HashSet<AuthorityName> = self
                .committee
                .voting_rights
                .iter()
                .map(|(name, _)| *name)
                .collect();

            let (aggregate_object_info, certificates) = self
                .get_object_by_id(*object_id, timeout_after_quorum)
                .await?;

            let mut aggregate_object_info: Vec<_> = aggregate_object_info.into_iter().collect();

            // If more that one version of an object is available, we update all authorities with it.
            while !aggregate_object_info.is_empty() {
                // This will be the very latest object version, because object_ref is ordered this way.
                let ((object_ref, transaction_digest), (object_option, object_authorities)) =
                    aggregate_object_info.pop().unwrap(); // safe due to check above

                // NOTE: Here we must check that the object is indeed an input to this transaction
                //       but for the moment lets do the happy case.

                if !certificates.contains_key(&transaction_digest) {
                    // NOTE: This implies this is a genesis object. We should check that it is.
                    //       We can do this by looking into the genesis, or the object_refs of the genesis.
                    //       Otherwise report the authority as potentially faulty.

                    if let Some(obj) = object_option {
                        active_objects.push((obj, None));
                    }
                    // Cannot be that the genesis contributes to deleted objects

                    continue;
                }

                let cert = certificates[&transaction_digest].clone(); // safe due to check above.

                // Remove authorities at this version, they will not need to be updated.
                for (name, _signed_order) in object_authorities {
                    authorites.remove(&name);
                }

                // NOTE: Just above we have access to signed orders that have not quite
                //       been processed by enough authorities. We should either return them
                //       to the caller, or -- more in the spirit of this function -- do what
                //       needs to be done to force their processing if this is possible.

                // Add authorities that need to be updated
                let entry = certs_to_sync
                    .entry(cert.order.digest())
                    .or_insert((cert.clone(), HashSet::new()));
                entry.1.extend(authorites);

                // Return the latest version of an object, or a deleted object
                match object_option {
                    Some(obj) => active_objects.push((obj, Some(cert))),
                    None => deleted_objects.push((object_ref, Some(cert))),
                }

                break;
            }
        }

        for (_, (cert, authorities)) in certs_to_sync {
            for name in authorities {
                // For each certificate authority pair run a sync to upate this authority to this
                // certificate.
                // NOTE: this is right now done sequentially, we should do them in parallel using
                //       the usual FuturesUnordered.
                let _result = self
                    .sync_certificate_to_authority_with_timeout(
                        ConfirmationOrder::new(cert.clone()),
                        name,
                        timeout_after_quorum,
                        DEFAULT_RETRIES,
                    )
                    .await;

                // TODO: collect errors and propagate them to the right place
            }
        }

        Ok((active_objects, deleted_objects))
    }

    /// Ask authorities for the user owned objects. Then download all objects at all versions present
    /// on authorites, along with the certificates preceeding them, and update lagging authorities to
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
            Vec<(Object, Option<CertifiedOrder>)>,
            Vec<(ObjectRef, Option<CertifiedOrder>)>,
        ),
        SuiError,
    > {
        // Contact a quorum of authorities, and return all objects they report we own.
        let (object_map, _authority_list) = self
            .get_all_owned_objects(address, timeout_after_quorum)
            .await?;

        let all_object_ids: HashSet<_> = object_map.keys().map(|object_ref| object_ref.0).collect();

        // Then sync all the owned objects
        self.sync_all_given_objects(
            &all_object_ids.into_iter().collect::<Vec<_>>(),
            timeout_after_quorum,
        )
        .await
    }

    /// Takes an order, brings all authorities up to date with the versions of the
    /// objects needed, and then submits the order to make a certificate.
    pub async fn process_order(
        &self,
        order: Order,
        timeout_after_quorum: Duration,
    ) -> Result<CertifiedOrder, SuiError> {
        // Find out which objects are required by this order and
        // ensure they are synced on authorities.
        let required_ids: Vec<ObjectID> = order
            .input_objects()
            .iter()
            .map(|o| o.object_id())
            .collect();

        let (_active_objects, _deleted_objects) = self
            .sync_all_given_objects(&required_ids, timeout_after_quorum)
            .await?;

        // Now broadcast the order to all authorities.
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();

        struct ProcessOrderState {
            // The list of signatures gathered at any point
            signatures: Vec<(AuthorityName, Signature)>,
            // A certificate if we manage to make or find one
            certificate: Option<CertifiedOrder>,
            // The list of errors gathered at any point
            errors: Vec<SuiError>,
            // Tally of stake for good vs bad responses.
            good_stake: usize,
            bad_stake: usize,
        }

        let state = ProcessOrderState {
            signatures: vec![],
            certificate: None,
            errors: vec![],
            good_stake: 0,
            bad_stake: 0,
        };

        let order_ref = &order;
        let state = self
            .quorum_map_then_reduce_with_timeout(
                state,
                |_name, client| {
                    Box::pin(async move { client.handle_order(order_ref.clone()).await })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        match result {
                            // If we are given back a certificate, then we do not need
                            // to re-submit this order, we just returned the ready made
                            // certificate.
                            Ok(OrderInfoResponse {
                                certified_order: Some(inner_certificate),
                                ..
                            }) => {
                                state.certificate = Some(inner_certificate);
                            }

                            // If we get back a signed order, then we aggregate the
                            // new signature and check whether we have enough to form
                            // a certificate.
                            Ok(OrderInfoResponse {
                                signed_order: Some(inner_signed_order),
                                ..
                            }) => {
                                state.signatures.push((name, inner_signed_order.signature));
                                state.good_stake += weight;
                                if state.good_stake >= threshold {
                                    state.certificate = Some(CertifiedOrder {
                                        order: order_ref.clone(),
                                        signatures: state.signatures.clone(),
                                    });
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
                                if state.bad_stake > validity {
                                    // Too many errors
                                    return Err(SuiError::QuorumNotReached {
                                        errors: state.errors,
                                    });
                                }
                            }
                            // In case we don't get an error but also don't get a valid value
                            _ => {
                                state.errors.push(SuiError::ErrorWhileProcessingOrder);
                                state.bad_stake += weight; // This is the bad stake counter
                                if state.bad_stake > validity {
                                    // Too many errors
                                    return Err(SuiError::QuorumNotReached {
                                        errors: state.errors,
                                    });
                                }
                            }
                        };

                        // If we have a certificate, then finish, otherwise continue.
                        if state.certificate.is_some() {
                            Ok(ReduceOutput::End(state))
                        } else {
                            Ok(ReduceOutput::Continue(state))
                        }
                    })
                },
                // A long timeout before we hear back from a quorum
                Duration::from_secs(60),
            )
            .await?;

        // If we have some certificate return it, or return an error.
        state.certificate.ok_or(SuiError::ErrorWhileProcessingOrder)
    }

    /// Process a certificate assuming that 2f+1 authorites already are up to date.
    ///
    /// This call is meant to be called after `process_order` returns a certificate.
    /// At that point (and after) enough authorities are up to date with all objects
    /// needed to process the certificate that a submission should succeed. However,
    /// in case an authority returns an error, we do try to bring it up to speed.
    pub async fn process_certificate(
        &self,
        certificate: CertifiedOrder,
        timeout_after_quorum: Duration,
    ) -> Result<OrderEffects, SuiError> {
        struct ProcessCertificateState {
            effects_map: HashMap<[u8; 32], (usize, OrderEffects)>,
            bad_stake: usize,
        }

        let state = ProcessCertificateState {
            effects_map: HashMap::new(),
            bad_stake: 0,
        };

        let cert_ref = &certificate;
        let threshold = self.committee.quorum_threshold();
        let validity = self.committee.validity_threshold();
        let state = self
            .quorum_map_then_reduce_with_timeout(
                state,
                |_name, client| {
                    Box::pin(async move {
                        // Here is the per-authority logic to process a certificate:
                        // - we try to process a cert, and return Ok on success.
                        // - we try to update the authority with the cert, and on error return Err.
                        // - we try to re-process the certificate and return the result.

                        let res = client
                            .handle_confirmation_order(ConfirmationOrder::new(cert_ref.clone()))
                            .await;

                        if res.is_ok() {
                            // We got an ok answer, so returning the result of processing
                            // the order.
                            return res;
                        }

                        // If we got an error, we try to update the authority.
                        let _result = self
                            .sync_certificate_to_authority_with_timeout(
                                ConfirmationOrder::new(cert_ref.clone()),
                                _name,
                                timeout_after_quorum,
                                DEFAULT_RETRIES,
                            )
                            .await?;

                        // Now try again
                        client
                            .handle_confirmation_order(ConfirmationOrder::new(cert_ref.clone()))
                            .await
                    })
                },
                |mut state, _name, weight, result| {
                    Box::pin(async move {
                        // We aggregate the effects response, until we have more than 2f
                        // and return.
                        if let Ok(OrderInfoResponse {
                            signed_effects: Some(inner_effects),
                            ..
                        }) = result
                        {
                            // Note: here we aggregate votes by the hash of the effects structure
                            let entry = state
                                .effects_map
                                .entry(sha3_hash(&inner_effects.effects))
                                .or_insert((0usize, inner_effects.effects));
                            entry.0 += weight;

                            if entry.0 >= threshold {
                                // It will set the timeout quite high.
                                return Ok(ReduceOutput::ContinueWithTimeout(
                                    state,
                                    timeout_after_quorum,
                                ));
                            }
                        }

                        // If instead we have more than f bad responses, then we fail.
                        state.bad_stake += weight;
                        if state.bad_stake > validity {
                            return Err(SuiError::ErrorWhileRequestingCertificate);
                        }

                        Ok(ReduceOutput::Continue(state))
                    })
                },
                // A long timeout before we hear back from a quorum
                Duration::from_secs(60),
            )
            .await?;

        // Check that one effects structure has more than 2f votes,
        // and return it.
        for (stake, effects) in state.effects_map.into_values() {
            if stake >= threshold {
                return Ok(effects);
            }
        }

        // If none has, fail.
        Err(SuiError::ErrorWhileRequestingCertificate)
    }

    #[cfg(test)]
    async fn request_certificate(
        &self,
        _sender: SuiAddress,
        object_id: ObjectID,
        _sequence_number: SequenceNumber,
    ) -> Result<CertifiedOrder, SuiError> {
        let (object_map, transaction_map) = self
            .get_object_by_id(object_id, Duration::from_secs(10))
            .await?;

        let (_obj_ref, tx_digest) = object_map.keys().last().unwrap();
        Ok(transaction_map[tx_digest].clone())
    }

    /// Find the highest sequence number that is known to a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_latest_sequence_number(&self, object_id: ObjectID) -> SequenceNumber {
        let (object_infos, _certificates) = self
            .get_object_by_id(object_id, Duration::from_secs(60))
            .await
            .unwrap(); // Not safe, but want to blow up if testing.
        let top_ref = object_infos.keys().last().unwrap().0;
        top_ref.1
    }

    /// Return owner address and sequence number of an object backed by a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_latest_owner(&self, object_id: ObjectID) -> (Authenticator, SequenceNumber) {
        let (object_infos, _certificates) = self
            .get_object_by_id(object_id, Duration::from_secs(60))
            .await
            .unwrap(); // Not safe, but want to blow up if testing.
        let (top_ref, obj) = object_infos.iter().last().unwrap();
        (obj.0.as_ref().unwrap().owner, top_ref.0 .1)
    }

    pub async fn execute_transaction(
        &self,
        order: &Order,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error> {
        let new_certificate = self
            .process_order(order.clone(), Duration::from_secs(60))
            .await?;
        let response = self
            .process_certificate(new_certificate.clone(), Duration::from_secs(60))
            .await?;

        Ok((new_certificate, response))
    }

    pub async fn get_object_info_execute(
        &mut self,
        object_id: ObjectID,
    ) -> Result<ObjectRead, anyhow::Error> {
        let (object_map, cert_map) = self
            .get_object_by_id(object_id, AUTHORITY_REQUEST_TIMEOUT)
            .await?;
        let mut object_ref_stack: Vec<_> = object_map.into_iter().collect();

        while let Some(((obj_ref, tx_digest), (obj_option, authorities))) = object_ref_stack.pop() {
            let stake: usize = authorities
                .iter()
                .map(|(name, _)| self.committee.weight(name))
                .sum();

            // If we have f+1 stake telling us of the latest version of the object, we just accept it.
            if stake >= self.committee.validity_threshold() {
                if obj_option.is_none() {
                    return Ok(ObjectRead::Deleted(obj_ref));
                } else {
                    // safe due to check
                    return Ok(ObjectRead::Exists(obj_ref, obj_option.unwrap()));
                }
            } else if cert_map.contains_key(&tx_digest) {
                // If we have less stake telling us about the latest state of an object
                // we re-run the certificate on all authorities to ensure it is correct.
                if let Ok(_effects) = self
                    .process_certificate(cert_map[&tx_digest].clone(), AUTHORITY_REQUEST_TIMEOUT)
                    .await
                {
                    let mut is_ok = false;

                    // The mutated or created case
                    if _effects
                        .mutated
                        .iter()
                        .filter(|(oref, _)| *oref == obj_ref)
                        .count()
                        != 0
                        || _effects
                            .created
                            .iter()
                            .filter(|(oref, _)| *oref == obj_ref)
                            .count()
                            != 0
                    {
                        is_ok = true;
                    }

                    // The deleted case
                    if obj_ref.2 == OBJECT_DIGEST_DELETED
                        && _effects
                            .deleted
                            .iter()
                            .filter(|(id, seq, _)| *id == obj_ref.0 && seq.increment() == obj_ref.1)
                            .count()
                            != 0
                    {
                        is_ok = true;
                    }

                    if !is_ok {
                        // Report a byzantine fault here
                        continue;
                    }

                    // NOTE: here we should validate the object is correct from the effects
                    if obj_option.is_none() {
                        return Ok(ObjectRead::Deleted(obj_ref));
                    } else {
                        // safe due to check
                        return Ok(ObjectRead::Exists(obj_ref, obj_option.unwrap()));
                    }
                }
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
                AUTHORITY_REQUEST_TIMEOUT,
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
        let request = ObjectInfoRequest {
            object_id,
            request_sequence_number: None,
        };

        // For now assume all authorities. Assume they're all honest
        // This assumption is woeful, and should be fixed
        // TODO: https://github.com/MystenLabs/fastnft/issues/320
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
                // did the response match the digest?
                Some(o) if o.object.digest() == object_ref.2 => {
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
}
