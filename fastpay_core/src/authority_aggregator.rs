// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::{authority_client::AuthorityAPI, downloader::*};
use async_trait::async_trait;
use fastx_types::object::Object;
use fastx_types::{
    base_types::*,
    committee::Committee,
    error::{FastPayError, FastPayResult},
    fp_ensure,
    messages::*,
};
use futures::{future, StreamExt, TryFutureExt};
use rand::seq::SliceRandom;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;

// TODO: Make timeout duration configurable.
const AUTHORITY_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

const OBJECT_DOWNLOAD_CHANNEL_BOUND: usize = 1024;

#[cfg(test)]
#[path = "unit_tests/client_tests.rs"]
mod client_tests;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub struct AuthorityAggregator<AuthorityAPI> {
    /// Our FastPay committee.
    pub committee: Committee,
    /// How to talk to this committee.
    authority_clients: BTreeMap<AuthorityName, AuthorityAPI>,
}

impl<AuthorityAPI> AuthorityAggregator<AuthorityAPI> {
    pub fn new(
        committee: Committee,
        authority_clients: BTreeMap<AuthorityName, AuthorityAPI>,
    ) -> Self {
        Self {
            committee,
            authority_clients,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
struct CertificateRequester<A> {
    committee: Committee,
    authority_clients: Vec<A>,
    sender: Option<FastPayAddress>,
}

impl<A> CertificateRequester<A> {
    fn new(
        committee: Committee,
        authority_clients: Vec<A>,
        sender: Option<FastPayAddress>,
    ) -> Self {
        Self {
            committee,
            authority_clients,
            sender,
        }
    }
}

#[async_trait]
impl<A> Requester for CertificateRequester<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    type Key = (ObjectID, SequenceNumber);
    type Value = Result<CertifiedOrder, FastPayError>;

    /// Try to find a certificate for the given sender, object_id and sequence number.
    async fn query(
        &mut self,
        (object_id, sequence_number): (ObjectID, SequenceNumber),
    ) -> Result<CertifiedOrder, FastPayError> {
        // BUG(https://github.com/MystenLabs/fastnft/issues/290): This function assumes that requesting the parent cert of object seq+1 will give the cert of
        //        that creates the object. This is not true, as objects may be deleted and may not have a seq+1
        //        to look up.
        //
        //        The authority `handle_object_info_request` is now fixed to return the parent at seq, and not
        //        seq+1. But a lot of the client code makes the above wrong assumption, and the line above reverts
        //        query to the old (incorrect) behavious to not break tests everywhere.
        let inner_sequence_number = sequence_number.increment();

        let request = ObjectInfoRequest {
            object_id,
            request_sequence_number: Some(inner_sequence_number),
        };
        // Sequentially try each authority in random order.
        // TODO: Improve shuffle, different authorities might different amount of stake.
        self.authority_clients.shuffle(&mut rand::thread_rng());
        for client in self.authority_clients.iter_mut() {
            let result = client.handle_object_info_request(request.clone()).await;
            if let Ok(response) = result {
                let certificate = response
                    .parent_certificate
                    .expect("Unable to get certificate");
                if certificate.check(&self.committee).is_ok() {
                    // BUG (https://github.com/MystenLabs/fastnft/issues/290): Orders do not have a sequence number any more, objects do.
                    /*
                    let order = &certificate.order;
                    if let Some(sender) = self.sender {

                        if order.sender() == &sender && order.sequence_number() == inner_sequence_number {
                            return Ok(certificate.clone());
                        }
                    } else {
                        return Ok(certificate.clone());
                    }
                    */
                    return Ok(certificate);
                }
            }
        }
        Err(FastPayError::ErrorWhileRequestingCertificate)
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
    async fn sync_authority_source_to_destination(
        &self,
        cert: ConfirmationOrder,
        source_authority: AuthorityName,
        destination_authority: AuthorityName,
    ) -> Result<(), FastPayError> {
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
                Err(FastPayError::LockErrors { .. }) => {}
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
                return Err(FastPayError::AuthorityInformationUnavailable);
            }
            attempted_certificates.insert(cert_digest);

            // TODO: Eventually the client will store more information, and we could
            // first try to read certificates and parents from a local cache before
            // asking an authority.
            // let input_objects = target_cert.certificate.order.input_objects();

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
                .ok_or(FastPayError::AuthorityInformationUnavailable)?;

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
                        .ok_or(FastPayError::AuthorityInformationUnavailable)?;

                    // Check & Add it to the list of certificates to sync
                    returned_certificate.check(&self.committee).map_err(|_| {
                        FastPayError::ByzantineAuthoritySuspicion {
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
    pub async fn sync_certificate_to_authority_with_timeout(
        &self,
        cert: ConfirmationOrder,
        destination_authority: AuthorityName,
        timeout_milliseconds: u64,
        retries: usize,
    ) -> Result<(), FastPayError> {
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
                Duration::from_millis(timeout_milliseconds),
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
        Err(FastPayError::AuthorityUpdateFailure)
    }

    #[cfg(test)]
    async fn request_certificate(
        &self,
        sender: FastPayAddress,
        object_id: ObjectID,
        sequence_number: SequenceNumber,
    ) -> Result<CertifiedOrder, FastPayError> {
        CertificateRequester::new(
            self.committee.clone(),
            self.authority_clients.values().cloned().collect(),
            Some(sender),
        )
        .query((object_id, sequence_number))
        .await
    }

    /// Find the highest sequence number that is known to a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_strong_majority_sequence_number(&self, object_id: ObjectID) -> SequenceNumber {
        let request = ObjectInfoRequest {
            object_id,
            request_sequence_number: None,
        };
        let mut authority_clients = self.authority_clients.clone();
        let numbers: futures::stream::FuturesUnordered<_> = authority_clients
            .iter_mut()
            .map(|(name, client)| {
                let fut = client.handle_object_info_request(request.clone());
                async move {
                    match fut.await {
                        Ok(info) => info.object().map(|obj| (*name, obj.version())),
                        _ => None,
                    }
                }
            })
            .collect();
        self.committee.get_strong_majority_lower_bound(
            numbers.filter_map(|x| async move { x }).collect().await,
        )
    }

    /// Return owner address and sequence number of an object backed by a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_strong_majority_owner(
        &self,
        object_id: ObjectID,
    ) -> Option<(Authenticator, SequenceNumber)> {
        let request = ObjectInfoRequest {
            object_id,
            request_sequence_number: None,
        };
        let authority_clients = self.authority_clients.clone();
        let numbers: futures::stream::FuturesUnordered<_> = authority_clients
            .iter()
            .map(|(name, client)| {
                let fut = client.handle_object_info_request(request.clone());
                async move {
                    match fut.await {
                        Ok(ObjectInfoResponse {
                            object_and_lock: Some(ObjectResponse { object, .. }),
                            ..
                        }) => Some((*name, Some((object.owner, object.version())))),
                        _ => None,
                    }
                }
            })
            .collect();
        self.committee.get_strong_majority_lower_bound(
            numbers.filter_map(|x| async move { x }).collect().await,
        )
    }

    /// Execute a sequence of actions in parallel for a quorum of authorities.
    async fn communicate_with_quorum<'a, V, F>(&'a self, execute: F) -> Result<Vec<V>, FastPayError>
    where
        F: Fn(AuthorityName, &'a A) -> AsyncResult<'a, V, FastPayError> + Clone,
    {
        let committee = &self.committee;
        let authority_clients = &self.authority_clients;
        let mut responses: futures::stream::FuturesUnordered<_> = authority_clients
            .iter()
            .map(|(name, client)| {
                let execute = execute.clone();
                async move { (*name, execute(*name, client).await) }
            })
            .collect();

        let mut values = Vec::new();
        let mut value_score = 0;
        let mut error_scores = HashMap::new();
        while let Some((name, result)) = responses.next().await {
            match result {
                Ok(value) => {
                    values.push(value);
                    value_score += committee.weight(&name);
                    if value_score >= committee.quorum_threshold() {
                        // Success!
                        return Ok(values);
                    }
                }
                Err(err) => {
                    let entry = error_scores.entry(err.clone()).or_insert(0);
                    *entry += committee.weight(&name);
                    if *entry >= committee.validity_threshold() {
                        // At least one honest node returned this error.
                        // No quorum can be reached, so return early.
                        return Err(FastPayError::QuorumNotReached {
                            errors: error_scores.into_keys().collect(),
                        });
                    }
                }
            }
        }
        Err(FastPayError::QuorumNotReached {
            errors: error_scores.into_keys().collect(),
        })
    }

    /// Broadcast missing confirmation orders and invoke handle_order on each authority client.
    async fn broadcast_and_handle_order(
        &self,
        order: Order,
    ) -> Result<(Vec<(CertifiedOrder, OrderInfoResponse)>, CertifiedOrder), anyhow::Error> {
        let committee = self.committee.clone();
        let (responses, votes) = self
            .broadcast_and_execute(Vec::new(), |name, authority| {
                let order = order.clone();
                let committee = committee.clone();
                Box::pin(async move {
                    match authority.handle_order(order).await {
                        Ok(OrderInfoResponse {
                            signed_order: Some(inner_signed_order),
                            ..
                        }) => {
                            fp_ensure!(
                                inner_signed_order.authority == name,
                                FastPayError::ErrorWhileProcessingTransferOrder
                            );
                            inner_signed_order.check(&committee)?;
                            Ok((inner_signed_order.authority, inner_signed_order.signature))
                        }
                        Err(err) => Err(err),
                        _ => Err(FastPayError::ErrorWhileProcessingTransferOrder),
                    }
                })
            })
            .await?;
        let certificate = CertifiedOrder {
            order,
            signatures: votes,
        };
        // Certificate is valid because
        // * `communicate_with_quorum` ensured a sufficient "weight" of (non-error) answers were returned by authorities.
        // * each answer is a vote signed by the expected authority.
        Ok((responses, certificate))
    }

    /// Broadcast missing confirmation orders and execute provided authority action on each authority.
    // BUG(https://github.com/MystenLabs/fastnft/issues/290): This logic for
    // updating an authority that is behind is not correct, since we now have
    // potentially many dependencies that need to be satisfied, not just a
    // list.
    async fn broadcast_and_execute<'a, V, F: 'a>(
        &'a self,
        certificates_to_broadcast: Vec<CertifiedOrder>,
        action: F,
    ) -> Result<(Vec<(CertifiedOrder, OrderInfoResponse)>, Vec<V>), anyhow::Error>
    where
        F: Fn(AuthorityName, &'a A) -> AsyncResult<'a, V, FastPayError> + Send + Sync + Copy,
        V: Copy,
    {
        let result = self
            .communicate_with_quorum(|name, client| {
                let certificates_to_broadcast = certificates_to_broadcast.clone();
                Box::pin(async move {
                    let mut responses = vec![];
                    for certificate in certificates_to_broadcast {
                        responses.push((
                            certificate.clone(),
                            client
                                .handle_confirmation_order(ConfirmationOrder::new(certificate))
                                .await?,
                        ));
                    }
                    Ok((responses, action(name, client).await?))
                })
            })
            .await?;

        let action_results = result.iter().map(|(_, result)| *result).collect();

        // Assume all responses are the same, pick the first one.
        let order_response = result
            .iter()
            .map(|(response, _)| response.clone())
            .next()
            .unwrap_or_default();

        Ok((order_response, action_results))
    }

    pub async fn update_authority_certificates(
        &mut self,
        sender: FastPayAddress,
        inputs: &[InputObjectKind],
        known_certificates: Vec<((ObjectID, SequenceNumber), FastPayResult<CertifiedOrder>)>,
    ) -> FastPayResult<Vec<Vec<(CertifiedOrder, OrderInfoResponse)>>> {
        let requester = CertificateRequester::new(
            self.committee.clone(),
            self.authority_clients.values().cloned().collect(),
            Some(sender),
        );

        let (_, handle) = Downloader::start(requester, known_certificates);
        self.communicate_with_quorum(|_name, client| {
            let mut handle = handle.clone();
            Box::pin(async move {
                // Sync certificate with authority
                // Figure out which certificates this authority is missing.
                let mut responses = Vec::new();
                let mut missing_certificates = Vec::new();
                for input_kind in inputs {
                    let object_id = input_kind.object_id();
                    let target_sequence_number = input_kind.version();
                    let request = ObjectInfoRequest {
                        object_id,
                        request_sequence_number: None,
                    };
                    let response = client.handle_object_info_request(request).await?;

                    let current_sequence_number = response
                        .object_and_lock
                        .ok_or(FastPayError::ObjectNotFound { object_id })?
                        .object
                        .version();

                    // Download each missing certificate in reverse order using the downloader.
                    let mut number = target_sequence_number.decrement();
                    while let Ok(seq) = number {
                        if seq < current_sequence_number {
                            break;
                        }
                        let certificate = handle
                            .query((object_id, seq))
                            .await
                            .map_err(|_| FastPayError::ErrorWhileRequestingCertificate)??;
                        missing_certificates.push(certificate);
                        number = seq.decrement();
                    }
                }

                // Send all missing confirmation orders.
                missing_certificates.reverse();
                for certificate in missing_certificates {
                    responses.push((
                        certificate.clone(),
                        client
                            .handle_confirmation_order(ConfirmationOrder::new(certificate))
                            .await?,
                    ));
                }
                Ok(responses)
            })
        })
        .await
    }

    /// Broadcast confirmation orders.
    /// The corresponding sequence numbers should be consecutive and increasing.
    pub async fn broadcast_confirmation_orders(
        &self,
        certificates_to_broadcast: Vec<CertifiedOrder>,
    ) -> Result<Vec<(CertifiedOrder, OrderInfoResponse)>, anyhow::Error> {
        self.broadcast_and_execute(certificates_to_broadcast, |_, _| Box::pin(async { Ok(()) }))
            .await
            .map(|(responses, _)| responses)
    }

    pub async fn request_certificates_from_authority(
        &self,
        known_sequence_numbers_map: BTreeMap<(ObjectID, SequenceNumber), HashSet<SequenceNumber>>,
    ) -> Result<BTreeMap<ObjectID, Vec<CertifiedOrder>>, FastPayError> {
        let mut sent_certificates: BTreeMap<ObjectID, Vec<CertifiedOrder>> = BTreeMap::new();

        for ((object_id, next_sequence_number), known_sequence_numbers) in
            known_sequence_numbers_map
        {
            let mut requester = CertificateRequester::new(
                self.committee.clone(),
                self.authority_clients.values().cloned().collect(),
                None,
            );

            let entry = sent_certificates.entry(object_id).or_default();
            // TODO: it's inefficient to loop through sequence numbers to retrieve missing cert, rethink this logic when we change certificate storage in client.
            let mut number = SequenceNumber::from(0);
            while number < next_sequence_number {
                if !known_sequence_numbers.contains(&number) {
                    let certificate = requester.query((object_id, number)).await?;
                    entry.push(certificate);
                }
                number = number.increment();
            }
        }
        Ok(sent_certificates)
    }

    pub async fn execute_transaction(
        &self,
        order: &Order,
    ) -> Result<(CertifiedOrder, OrderInfoResponse), anyhow::Error> {
        let new_certificate = self.execute_transaction_without_confirmation(order).await?;

        // Confirm last transfer certificate if needed.
        let responses = self
            .broadcast_confirmation_orders(vec![new_certificate.clone()])
            .await?;

        // Find response for the current order from all the returned order responses.
        let (_, response) = responses
            .into_iter()
            .find(|(cert, _)| cert.order == new_certificate.order)
            .ok_or(FastPayError::ErrorWhileRequestingInformation)?;

        Ok((new_certificate, response))
    }

    /// Execute (or retry) an order without confirmation. Update local object states using newly created certificate.
    pub async fn execute_transaction_without_confirmation(
        &self,
        order: &Order,
    ) -> Result<CertifiedOrder, anyhow::Error> {
        let result = self.broadcast_and_handle_order(order.clone()).await;

        // order_info_response contains response from broadcasting old unconfirmed order, if any.
        let (_order_info_responses, new_sent_certificate) = result?;
        assert_eq!(&new_sent_certificate.order, order);
        // TODO: Verify that we don't need to update client objects here based on _order_info_responses,
        // but can do it at the caller site.

        Ok(new_sent_certificate)
    }

    // TODO: This is incomplete at the moment.
    // A complete algorithm is being introduced in
    // https://github.com/MystenLabs/fastnft/pull/336.
    pub async fn download_own_object_ids_from_random_authority(
        &self,
        address: FastPayAddress,
    ) -> Result<(AuthorityName, Vec<ObjectRef>), FastPayError> {
        let request = AccountInfoRequest { account: address };
        // Sequentially try each authority in random order.
        let mut authorities: Vec<&AuthorityName> = self.authority_clients.keys().collect();
        // TODO: implement sampling according to stake distribution and using secure RNG. https://github.com/MystenLabs/fastnft/issues/128
        authorities.shuffle(&mut rand::thread_rng());
        // Authority could be byzantine, add timeout to avoid waiting forever.
        for authority_name in authorities {
            let authority = self.authority_clients.get(authority_name).unwrap();
            let result = timeout(
                AUTHORITY_REQUEST_TIMEOUT,
                authority.handle_account_info_request(request.clone()),
            )
            .map_err(|_| FastPayError::ErrorWhileRequestingInformation)
            .await?;
            if let Ok(AccountInfoResponse { object_ids, .. }) = &result {
                return Ok((*authority_name, object_ids.clone()));
            }
        }
        Err(FastPayError::ErrorWhileRequestingInformation)
    }

    pub async fn get_object_info_execute(
        &mut self,
        object_info_req: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, anyhow::Error> {
        let votes = self
            .communicate_with_quorum(|_, client| {
                let req = object_info_req.clone();
                Box::pin(async move { client.handle_object_info_request(req).await })
            })
            .await?;

        votes
            .get(0)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No valid confirmation order votes"))
    }

    /// Given a list of object refs, download the objects.
    pub fn fetch_objects_from_authorities(
        &self,
        object_refs: BTreeSet<ObjectRef>,
    ) -> Receiver<FastPayResult<Object>> {
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
        authority_clients: BTreeMap<PublicKeyBytes, A>,
        object_ref: ObjectRef,
        timeout: Duration,
        sender: tokio::sync::mpsc::Sender<Result<Object, FastPayError>>,
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

        fn obj_fetch_err(id: ObjectID, err: &str) -> Result<Object, FastPayError> {
            Err(FastPayError::ObjectFetchFailed {
                object_id: id,
                err: err.to_owned(),
            })
        }

        let mut ret_val: Result<Object, FastPayError> = Err(FastPayError::ObjectFetchFailed {
            object_id: object_ref.0,
            err: "No authority returned object".to_string(),
        });
        // Find the first non-error value
        // There are multiple reasons why we might not have an object
        // We can timeout, or the authority returns an error or simply no object
        // When we get an object back, it also might not match the digest we want
        for result in results {
            // Check if the result of the call is successful
            ret_val = match result {
                Ok(res) => match res {
                    // Check if the authority actually had an object
                    Ok(resp) => match resp.object_and_lock {
                        Some(o) => {
                            // Check if this is the the object we want
                            if o.object.digest() == object_ref.2 {
                                Ok(o.object)
                            } else {
                                obj_fetch_err(object_id, "Object digest mismatch")
                            }
                        }
                        None => obj_fetch_err(object_id, "object_and_lock is None"),
                    },
                    // Something in FastX failed
                    Err(e) => Err(e),
                },
                // Took too long
                Err(e) => obj_fetch_err(object_id, e.to_string().as_str()),
            };
            // We found a value
            if ret_val.is_ok() {
                break;
            }
        }
        sender
            .send(ret_val)
            .await
            .expect("Cannot send object on channel after object fetch attempt");
    }
}
