// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::downloader::*;
use anyhow::ensure;
use async_trait::async_trait;
use fastx_framework::build_move_package_to_bytes;
use fastx_types::messages::Address::FastPay;
use fastx_types::{
    base_types::*, committee::Committee, error::FastPayError, fp_ensure, messages::*,
};
use futures::{future, StreamExt, TryFutureExt};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use rand::seq::SliceRandom;
use std::collections::btree_map::Entry;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;

#[cfg(test)]
#[path = "unit_tests/client_tests.rs"]
mod client_tests;

// TODO: Make timeout duration configurable.
const AUTHORITY_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

#[async_trait]
pub trait AuthorityClient {
    /// Initiate a new order to a FastPay or Primary account.
    async fn handle_order(&mut self, order: Order) -> Result<OrderInfoResponse, FastPayError>;

    /// Confirm an order to a FastPay or Primary account.
    async fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> Result<OrderInfoResponse, FastPayError>;

    /// Handle Account information requests for this account.
    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError>;

    /// Handle Object information requests for this account.
    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, FastPayError>;
}

pub struct ClientState<AuthorityClient> {
    /// Our FastPay address.
    address: FastPayAddress,
    /// Our signature key.
    secret: KeyPair,
    /// Our FastPay committee.
    committee: Committee,
    /// How to talk to this committee.
    authority_clients: HashMap<AuthorityName, AuthorityClient>,
    /// Pending transfer.
    pending_transfer: Option<Order>,

    // The remaining fields are used to minimize networking, and may not always be persisted locally.
    /// Known certificates, indexed by TX digest.
    certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
    /// The known objects with it's sequence number owned by the client.
    object_ids: BTreeMap<ObjectID, SequenceNumber>,

    object_certs: BTreeMap<ObjectID, Vec<TransactionDigest>>,
}

// Operations are considered successful when they successfully reach a quorum of authorities.
#[async_trait]
pub trait Client {
    /// Send object to a FastX account.
    async fn transfer_object(
        &mut self,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: FastPayAddress,
    ) -> Result<CertifiedOrder, anyhow::Error>;

    /// Receive object from FastX.
    async fn receive_object(&mut self, certificate: CertifiedOrder) -> Result<(), anyhow::Error>;

    /// Send object to a FastX account.
    /// Do not confirm the transaction.
    async fn transfer_to_fastx_unsafe_unconfirmed(
        &mut self,
        recipient: FastPayAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
    ) -> Result<CertifiedOrder, anyhow::Error>;

    /// Synchronise client state with a random authorities, updates all object_ids and certificates, request only goes out to one authority.
    /// this method doesn't guarantee data correctness, client will have to handle potential byzantine authority
    async fn sync_client_state_with_random_authority(
        &mut self,
    ) -> Result<AuthorityName, anyhow::Error>;

    /// Call move functions in the module in the given package, with args supplied
    async fn move_call(
        &mut self,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_object_ref: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error>;

    /// Publish Move modules
    async fn publish(
        &mut self,
        package_source_files_path: String,
        gas_object_ref: ObjectRef,
    ) -> Result<(CertifiedOrder, OrderEffects), FastPayError>;

    /// Get the object information
    async fn get_object_info(
        &mut self,
        object_info_req: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, anyhow::Error>;

    /// Get all object we own.
    async fn get_owned_objects(&self) -> Result<Vec<ObjectID>, anyhow::Error>;
}

impl<A> ClientState<A> {
    pub fn new(
        address: FastPayAddress,
        secret: KeyPair,
        committee: Committee,
        authority_clients: HashMap<AuthorityName, A>,
        certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
        object_ids: BTreeMap<ObjectID, SequenceNumber>,
    ) -> Self {
        Self {
            address,
            secret,
            committee,
            authority_clients,
            pending_transfer: None,
            certificates,
            object_ids,
            object_certs: BTreeMap::new(),
        }
    }

    pub fn address(&self) -> FastPayAddress {
        self.address
    }

    pub fn next_sequence_number(
        &self,
        object_id: &ObjectID,
    ) -> Result<SequenceNumber, FastPayError> {
        if self.object_ids.contains_key(object_id) {
            Ok(self.object_ids[object_id])
        } else {
            Err(FastPayError::ObjectNotFound)
        }
    }

    pub fn object_ids(&self) -> &BTreeMap<ObjectID, SequenceNumber> {
        &self.object_ids
    }

    pub fn pending_transfer(&self) -> &Option<Order> {
        &self.pending_transfer
    }

    pub fn certificates(&self, object_id: &ObjectID) -> impl Iterator<Item = &CertifiedOrder> {
        self.object_certs
            .get(object_id)
            .into_iter()
            .flat_map(|cert_digests| {
                cert_digests
                    .iter()
                    .filter_map(|digest| self.certificates.get(digest))
            })
    }

    pub fn all_certificates(&self) -> &BTreeMap<TransactionDigest, CertifiedOrder> {
        &self.certificates
    }
}

#[derive(Clone)]
struct CertificateRequester<A> {
    committee: Committee,
    authority_clients: Vec<A>,
    sender: Option<FastPayAddress>,
    object_id: ObjectID,
}

impl<A> CertificateRequester<A> {
    fn new(
        committee: Committee,
        authority_clients: Vec<A>,
        sender: Option<FastPayAddress>,
        object_id: ObjectID,
    ) -> Self {
        Self {
            committee,
            authority_clients,
            sender,
            object_id,
        }
    }
}

#[async_trait]
impl<A> Requester for CertificateRequester<A>
where
    A: AuthorityClient + Send + Sync + 'static + Clone,
{
    type Key = SequenceNumber;
    type Value = Result<CertifiedOrder, FastPayError>;

    /// Try to find a certificate for the given sender and sequence number.
    async fn query(
        &mut self,
        sequence_number: SequenceNumber,
    ) -> Result<CertifiedOrder, FastPayError> {
        let request = ObjectInfoRequest {
            object_id: self.object_id,
            request_sequence_number: Some(sequence_number),
            request_received_transfers_excluding_first_nth: None,
        };
        // Sequentially try each authority in random order.
        // TODO: Improve shuffle, different authorities might different amount of stake.
        self.authority_clients.shuffle(&mut rand::thread_rng());
        for client in self.authority_clients.iter_mut() {
            let result = client.handle_object_info_request(request.clone()).await;
            if let Ok(response) = result {
                let certificate = response.requested_certificate.unwrap();
                if certificate.check(&self.committee).is_ok() {
                    let order = &certificate.order;
                    if let Some(sender) = self.sender {
                        if order.sender() == &sender && order.sequence_number() == sequence_number {
                            return Ok(certificate.clone());
                        }
                    } else {
                        return Ok(certificate.clone());
                    }
                }
            }
        }
        Err(FastPayError::ErrorWhileRequestingCertificate)
    }
}

/// Used for communicate_transfers
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
enum CommunicateAction {
    SendOrder(Order),
    SynchronizeNextSequenceNumber(SequenceNumber),
}

impl<A> ClientState<A>
where
    A: AuthorityClient + Send + Sync + 'static + Clone,
{
    #[cfg(test)]
    async fn request_certificate(
        &mut self,
        sender: FastPayAddress,
        object_id: ObjectID,
        sequence_number: SequenceNumber,
    ) -> Result<CertifiedOrder, FastPayError> {
        CertificateRequester::new(
            self.committee.clone(),
            self.authority_clients.values().cloned().collect(),
            Some(sender),
            object_id,
        )
        .query(sequence_number)
        .await
    }

    /// Find the highest sequence number that is known to a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_strong_majority_sequence_number(&self, object_id: ObjectID) -> SequenceNumber {
        let request = ObjectInfoRequest {
            object_id,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        };
        let mut authority_clients = self.authority_clients.clone();
        let numbers: futures::stream::FuturesUnordered<_> = authority_clients
            .iter_mut()
            .map(|(name, client)| {
                let fut = client.handle_object_info_request(request.clone());
                async move {
                    match fut.await {
                        Ok(info) => Some((*name, info.object.version())),
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
    ) -> Option<(FastPayAddress, SequenceNumber)> {
        let request = ObjectInfoRequest {
            object_id,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        };
        let authority_clients = self.authority_clients.clone();
        let numbers: futures::stream::FuturesUnordered<_> = authority_clients
            .iter()
            .map(|(name, client)| {
                let fut = client.handle_object_info_request(request.clone());
                async move {
                    match fut.await {
                        Ok(info) => Some((*name, Some((info.object.owner, info.object.version())))),
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
    async fn communicate_with_quorum<'a, V, F>(
        &'a mut self,
        execute: F,
    ) -> Result<Vec<V>, FastPayError>
    where
        F: Fn(AuthorityName, &'a mut A) -> AsyncResult<'a, V, FastPayError> + Clone,
    {
        let committee = &self.committee;
        let authority_clients = &mut self.authority_clients;
        let mut responses: futures::stream::FuturesUnordered<_> = authority_clients
            .iter_mut()
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
                        return Err(FastPayError::FailedToCommunicateWithQuorum {
                            err: "quorum not reached".to_owned(),
                        });
                    }
                }
            }
        }
        Err(FastPayError::FailedToCommunicateWithQuorum {
            err: "multiple errors".to_string(),
        })
    }

    /// Broadcast confirmation orders and optionally one more transfer order.
    /// The corresponding sequence numbers should be consecutive and increasing.
    async fn communicate_transfers(
        &mut self,
        sender: FastPayAddress,
        object_id: ObjectID,
        known_certificates: Vec<CertifiedOrder>,
        action: CommunicateAction,
    ) -> Result<Vec<CertifiedOrder>, anyhow::Error> {
        let target_sequence_number = match &action {
            CommunicateAction::SendOrder(order) => order.sequence_number(),
            CommunicateAction::SynchronizeNextSequenceNumber(seq) => *seq,
        };
        let requester = CertificateRequester::new(
            self.committee.clone(),
            self.authority_clients.values().cloned().collect(),
            Some(sender),
            object_id,
        );
        let (task, mut handle) = Downloader::start(
            requester,
            known_certificates.into_iter().filter_map(|cert| {
                if cert.order.sender() == &sender {
                    Some((cert.order.sequence_number(), Ok(cert)))
                } else {
                    None
                }
            }),
        );
        let committee = self.committee.clone();
        let votes = self
            .communicate_with_quorum(|name, client| {
                let mut handle = handle.clone();
                let action = action.clone();
                let committee = &committee;
                Box::pin(async move {
                    // Figure out which certificates this authority is missing.
                    let request = ObjectInfoRequest {
                        object_id,
                        request_sequence_number: None,
                        request_received_transfers_excluding_first_nth: None,
                    };
                    let response = client.handle_object_info_request(request).await?;

                    let current_sequence_number = response.object.version();
                    // Download each missing certificate in reverse order using the downloader.
                    let mut missing_certificates = Vec::new();
                    let mut number = target_sequence_number.decrement();
                    while let Ok(value) = number {
                        if value < current_sequence_number {
                            break;
                        }
                        let certificate = handle
                            .query(value)
                            .await
                            .map_err(|_| FastPayError::ErrorWhileRequestingCertificate)??;
                        missing_certificates.push(certificate);
                        number = value.decrement();
                    }
                    // Send all missing confirmation orders.
                    missing_certificates.reverse();
                    for certificate in missing_certificates {
                        client
                            .handle_confirmation_order(ConfirmationOrder::new(certificate))
                            .await?;
                    }
                    // Send the transfer order (if any) and return a vote.
                    if let CommunicateAction::SendOrder(order) = action {
                        let result: Result<OrderInfoResponse, FastPayError> =
                            client.handle_order(order).await;
                        return match result {
                            Ok(OrderInfoResponse {
                                signed_order: Some(inner_signed_order),
                                ..
                            }) => {
                                fp_ensure!(
                                    inner_signed_order.authority == name,
                                    FastPayError::ErrorWhileProcessingTransferOrder
                                );
                                inner_signed_order.check(committee)?;
                                Ok(Some(inner_signed_order))
                            }
                            Err(err) => Err(err),
                            _ => Err(FastPayError::ErrorWhileProcessingTransferOrder),
                        };
                    }
                    Ok(None)
                })
            })
            .await?;
        // Terminate downloader task and retrieve the content of the cache.
        handle.stop().await?;
        let mut certificates: Vec<_> = task.await?.filter_map(Result::ok).collect();
        if let CommunicateAction::SendOrder(order) = action {
            let certificate = CertifiedOrder {
                order,
                signatures: votes
                    .into_iter()
                    .filter_map(|vote| match vote {
                        Some(signed_order) => {
                            Some((signed_order.authority, signed_order.signature))
                        }
                        None => None,
                    })
                    .collect(),
            };
            // Certificate is valid because
            // * `communicate_with_quorum` ensured a sufficient "weight" of (non-error) answers were returned by authorities.
            // * each answer is a vote signed by the expected authority.
            certificates.push(certificate);
        }
        Ok(certificates)
    }

    /// Make sure we have all our certificates with sequence number
    /// in the range 0..self.next_sequence_number
    pub async fn download_certificates(
        &mut self,
    ) -> Result<BTreeMap<ObjectID, Vec<CertifiedOrder>>, FastPayError> {
        let mut sent_certificates: BTreeMap<ObjectID, Vec<CertifiedOrder>> = BTreeMap::new();

        for (object_id, next_sequence_number) in self.object_ids.clone() {
            let known_sequence_numbers: BTreeSet<_> = self
                .certificates(&object_id)
                .flat_map(|cert| cert.order.input_objects())
                .filter_map(|(id, seq, _)| if id == object_id { Some(seq) } else { None })
                .collect();

            let mut requester = CertificateRequester::new(
                self.committee.clone(),
                self.authority_clients.values().cloned().collect(),
                None,
                object_id,
            );

            let entry = sent_certificates.entry(object_id).or_default();
            // TODO: it's inefficient to loop through sequence numbers to retrieve missing cert, rethink this logic when we change certificate storage in client.
            let mut number = SequenceNumber::from(0);
            while number < next_sequence_number {
                if !known_sequence_numbers.contains(&number) {
                    let certificate = requester.query(number).await?;
                    entry.push(certificate);
                }
                number = number.increment();
            }
        }
        Ok(sent_certificates)
    }

    /// Transfers an object to a recipient address.
    async fn transfer(
        &mut self,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: Address,
    ) -> Result<CertifiedOrder, anyhow::Error> {
        // TODO(https://github.com/MystenLabs/fastnft/issues/123): Include actual object digest here
        let object_ref = (
            object_id,
            self.next_sequence_number(&object_id)?,
            ObjectDigest::new([0; 32]),
        );
        let gas_payment = (
            gas_payment,
            self.next_sequence_number(&gas_payment)?,
            ObjectDigest::new([0; 32]),
        );

        let transfer = Transfer {
            object_ref,
            sender: self.address,
            recipient,
            gas_payment,
        };
        let order = Order::new_transfer(transfer, &self.secret);
        let certificate = self
            .execute_transfer(order, /* with_confirmation */ true)
            .await?;

        if let FastPay(address) = recipient {
            if address != self.address {
                self.object_certs.remove(&object_id);
                self.object_ids.remove(&object_id);
            }
        }

        Ok(certificate)
    }

    /// Update our view of certificates. Update the object_id and the next sequence number accordingly.
    /// NOTE: This is only useful in the eventuality of missing local data.
    /// We assume certificates to be valid and sent by us, and their sequence numbers to be unique.
    fn update_certificates(
        &mut self,
        object_id: &ObjectID,
        certificates: &[CertifiedOrder],
    ) -> Result<(), FastPayError> {
        for new_cert in certificates {
            // Try to get object's last seq number before the mutation, default to 0 for newly created object.
            let seq = new_cert
                .order
                .input_objects()
                .iter()
                .find_map(
                    |(id, seq, _)| {
                        if object_id == id {
                            Some(seq)
                        } else {
                            None
                        }
                    },
                )
                .cloned()
                .unwrap_or_default();

            let mut new_next_sequence_number = self.next_sequence_number(object_id)?;
            if seq >= new_next_sequence_number {
                new_next_sequence_number = seq.increment();
            }

            self.certificates
                .insert(new_cert.order.digest(), new_cert.clone());

            // Atomic update
            self.object_ids.insert(*object_id, new_next_sequence_number);

            let certs = self.object_certs.entry(*object_id).or_default();

            if !certs.contains(&new_cert.order.digest()) {
                certs.push(new_cert.order.digest());
            }
        }
        // Sanity check
        let certificates_count = self.certificates(object_id).count();
        assert_eq!(
            certificates_count,
            usize::from(self.next_sequence_number(object_id)?)
        );
        Ok(())
    }

    /// Execute (or retry) a transfer order. Update local balance.
    async fn execute_transfer(
        &mut self,
        order: Order,
        with_confirmation: bool,
    ) -> Result<CertifiedOrder, anyhow::Error> {
        ensure!(
            self.pending_transfer == None || self.pending_transfer.as_ref() == Some(&order),
            "Client state has a different pending transfer",
        );
        ensure!(
            order.sequence_number() == self.next_sequence_number(order.object_id())?,
            "Unexpected sequence number"
        );
        self.pending_transfer = Some(order.clone());
        let new_sent_certificates = self
            .communicate_transfers(
                self.address,
                *order.object_id(),
                self.certificates(order.object_id()).cloned().collect(),
                CommunicateAction::SendOrder(order.clone()),
            )
            .await?;
        assert_eq!(new_sent_certificates.last().unwrap().order, order);
        // Clear `pending_transfer` and update `sent_certificates`,
        // and `next_sequence_number`. (Note that if we were using persistent
        // storage, we should ensure update atomicity in the eventuality of a crash.)
        self.pending_transfer = None;

        // Only valid for object transfer, where input_objects = output_objects
        for (object_id, _, _) in order.input_objects() {
            self.update_certificates(&object_id, &new_sent_certificates)?;
        }
        // Confirm last transfer certificate if needed.
        if with_confirmation {
            self.communicate_transfers(
                self.address,
                *order.object_id(),
                self.certificates(order.object_id()).cloned().collect(),
                CommunicateAction::SynchronizeNextSequenceNumber(
                    self.next_sequence_number(order.object_id())?,
                ),
            )
            .await?;
        }
        // the object_certs has been updated by update_certificates above, .last().unwrap() should be safe here.
        Ok(self.certificates(order.object_id()).last().unwrap().clone())
    }

    async fn download_own_object_ids(
        &self,
    ) -> Result<(AuthorityName, Vec<ObjectRef>), FastPayError> {
        let request = AccountInfoRequest {
            account: self.address,
        };
        // Sequentially try each authority in random order.
        let mut authorities: Vec<AuthorityName> =
            self.authority_clients.clone().into_keys().collect();
        // TODO: implement sampling according to stake distribution and using secure RNG. https://github.com/MystenLabs/fastnft/issues/128
        authorities.shuffle(&mut rand::thread_rng());
        // Authority could be byzantine, add timeout to avoid waiting forever.
        for authority_name in authorities {
            let authority = self.authority_clients.get(&authority_name).unwrap();
            let result = timeout(
                AUTHORITY_REQUEST_TIMEOUT,
                authority.handle_account_info_request(request.clone()),
            )
            .map_err(|_| FastPayError::ErrorWhileRequestingInformation)
            .await?;
            if let Ok(AccountInfoResponse { object_ids, .. }) = &result {
                return Ok((authority_name, object_ids.clone()));
            }
        }
        Err(FastPayError::ErrorWhileRequestingInformation)
    }

    fn update_objects_from_order_info(
        &mut self,
        order_info_resp: OrderInfoResponse,
    ) -> Result<(), FastPayError> {
        if let Some(v) = order_info_resp.signed_effects {
            // The cert should be included in the response
            let cert = order_info_resp.certified_order.unwrap();
            let digest = cert.order.digest();
            self.certificates.insert(digest, cert);

            for &((object_id, seq, _), owner) in v.effects.all_mutated() {
                let old_seq = self.object_ids.get(&object_id).cloned().unwrap_or_default();
                fp_ensure!(
                    old_seq < seq,
                    FastPayError::UnexpectedSequenceNumber {
                        object_id,
                        expected_sequence: old_seq.increment(),
                        received_sequence: seq,
                    }
                );

                if owner == self.address {
                    self.object_ids.insert(object_id, seq);
                    self.object_certs.entry(object_id).or_default().push(digest);
                } else {
                    self.object_ids.remove(&object_id);
                    self.object_certs.remove(&object_id);
                }
            }

            for (obj_id, _, _) in v.effects.deleted {
                self.object_ids.remove(&obj_id);
                self.object_certs.remove(&obj_id);
            }
            Ok(())
        } else {
            Err(FastPayError::ErrorWhileRequestingInformation)
        }
    }
    /// TODO/TBD: Formalize how to handle failed transaction orders in FastX
    /// https://github.com/MystenLabs/fastnft/issues/174
    async fn communicate_transaction_order(
        &mut self,
        order: Order,
    ) -> Result<CertifiedOrder, FastPayError> {
        let committee = self.committee.clone();

        let votes = self
            .communicate_with_quorum(|name, client| {
                let order = order.clone();
                let committee = &committee;
                Box::pin(async move {
                    let result = client.handle_order(order).await;
                    let s_order = result
                        .as_ref()
                        .map(|order_info_resp| order_info_resp.signed_order.as_ref());
                    if let Ok(Some(signed_order)) = s_order {
                        fp_ensure!(
                            signed_order.authority == name,
                            FastPayError::ErrorWhileProcessingTransactionOrder {
                                err: format!(
                                    "Unexpected authority. Expected {:?}, got {:?}",
                                    name, signed_order.authority
                                )
                            }
                        );
                        signed_order.check(committee)?;
                        Ok(signed_order.clone())
                    } else {
                        Err(s_order.err().unwrap().clone())
                    }
                })
            })
            .await?;

        let certificate = CertifiedOrder {
            order: order.clone(),
            signatures: votes
                .iter()
                .map(|vote| (vote.authority, vote.signature))
                .collect(),
        };
        Ok(certificate)
    }

    /// TODO/TBD: Formalize how to handle failed transaction orders in FastX
    /// https://github.com/MystenLabs/fastnft/issues/174
    async fn communicate_confirmation_order(
        &mut self,
        cert_order: &CertifiedOrder,
    ) -> Result<OrderInfoResponse, FastPayError> {
        let committee = self.committee.clone();

        let votes = self
            .communicate_with_quorum(|name, client| {
                let certified_order = ConfirmationOrder {
                    certificate: cert_order.clone(),
                };
                let committee = &committee;
                Box::pin(async move {
                    let result = client.handle_confirmation_order(certified_order).await;

                    if let Ok(Some(signed_order)) = result
                        .as_ref()
                        .map(|order_info_resp| order_info_resp.signed_order.as_ref())
                    {
                        fp_ensure!(
                            signed_order.authority == name,
                            FastPayError::ErrorWhileProcessingConfirmationOrder {
                                err: format!(
                                    "Unexpected authority. Expected {:?}, got {:?}",
                                    name, signed_order.authority
                                )
                            }
                        );
                        signed_order.check(committee)?;
                        result
                    } else {
                        Err(result.err().unwrap())
                    }
                })
            })
            .await;

        match votes {
            Ok(mut v) => v
                .pop()
                .ok_or(FastPayError::ErrorWhileProcessingConfirmationOrder {
                    err: "Not enough confirmation votes: ".to_string(),
                }),
            Err(e) => Err(e),
        }
    }

    /// Execute call order
    async fn execute_call(
        &mut self,
        order: Order,
    ) -> Result<(CertifiedOrder, OrderEffects), FastPayError> {
        // Transaction order
        let new_certificate = self.communicate_transaction_order(order).await?;

        // Confirmation
        let order_info = self
            .communicate_confirmation_order(&new_certificate)
            .await?;

        // Update local object view
        self.update_objects_from_order_info(order_info.clone())?;

        let cert =
            order_info
                .certified_order
                .ok_or(FastPayError::ErrorWhileProcessingMoveCall {
                    err: "No certified orders returned from Move call operation".to_string(),
                })?;
        let effects = order_info
            .signed_effects
            .ok_or(FastPayError::ErrorWhileProcessingMoveCall {
                err: "No object info returned from Move call operation".to_string(),
            })?
            .effects;

        Ok((cert, effects))
    }

    /// Execute module publish
    async fn execute_publish(
        &mut self,
        order: Order,
    ) -> Result<(CertifiedOrder, OrderEffects), FastPayError> {
        // Transaction order
        let new_certificate = self.communicate_transaction_order(order).await?;

        // Confirmation
        let order_info = self
            .communicate_confirmation_order(&new_certificate)
            .await?;

        // Update local object view
        self.update_objects_from_order_info(order_info.clone())?;

        let cert = order_info
            .certified_order
            .ok_or(FastPayError::ErrorWhileProcessingPublish {
                err: "No certified orders returned from publish operation".to_string(),
            })?;
        let effects = order_info
            .signed_effects
            .ok_or(FastPayError::ErrorWhileProcessingPublish {
                err: "No object info returned from publish operation".to_string(),
            })?
            .effects;

        Ok((cert, effects))
    }

    async fn call(
        &mut self,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_object_ref: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error> {
        let move_call_order = Order::new_move_call(
            self.address,
            package_object_ref,
            module,
            function,
            type_arguments,
            gas_object_ref,
            object_arguments,
            pure_arguments,
            gas_budget,
            &self.secret,
        );

        Ok(self.execute_call(move_call_order).await?)
    }

    async fn publish(
        &mut self,
        package_source_files_path: String,
        gas_object_ref: ObjectRef,
    ) -> Result<(CertifiedOrder, OrderEffects), FastPayError> {
        // Try to compile the modules at the path into a package
        let compiled_modules =
            build_move_package_to_bytes(Path::new(&package_source_files_path), false)?;
        let move_publish_order =
            Order::new_module(self.address, gas_object_ref, compiled_modules, &self.secret);

        Ok(self.execute_publish(move_publish_order).await?)
    }

    async fn get_object_info_execute(
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
}

#[async_trait]
impl<A> Client for ClientState<A>
where
    A: AuthorityClient + Send + Sync + Clone + 'static,
{
    async fn transfer_object(
        &mut self,
        object_id: ObjectID,
        gas_payment: ObjectID,
        recipient: FastPayAddress,
    ) -> Result<CertifiedOrder, anyhow::Error> {
        self.transfer(object_id, gas_payment, Address::FastPay(recipient))
            .await
    }

    async fn receive_object(&mut self, certificate: CertifiedOrder) -> Result<(), anyhow::Error> {
        let _ = &certificate;
        match &certificate.order.kind {
            OrderKind::Transfer(transfer) => {
                ensure!(
                    transfer.recipient == Address::FastPay(self.address),
                    "Transfer should be received by us."
                );
                self.communicate_transfers(
                    transfer.sender,
                    *certificate.order.object_id(),
                    vec![certificate.clone()],
                    CommunicateAction::SynchronizeNextSequenceNumber(
                        transfer.object_ref.1.increment(),
                    ),
                )
                .await?;
                // Everything worked: update the local balance.
                if let Entry::Vacant(entry) = self.certificates.entry(certificate.order.digest()) {
                    self.object_ids
                        .insert(transfer.object_ref.0, transfer.object_ref.1.increment());
                    self.object_certs
                        .entry(transfer.object_ref.0)
                        .or_default()
                        .push(certificate.order.digest());
                    entry.insert(certificate);
                }
                Ok(())
            }
            OrderKind::Publish(_) | OrderKind::Call(_) => {
                unimplemented!("receiving (?) Move call or publish")
            }
        }
    }

    async fn transfer_to_fastx_unsafe_unconfirmed(
        &mut self,
        recipient: FastPayAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
    ) -> Result<CertifiedOrder, anyhow::Error> {
        let transfer = Transfer {
            object_ref: (
                object_id,
                self.next_sequence_number(&object_id)?,
                // TODO(https://github.com/MystenLabs/fastnft/issues/123): Include actual object digest here
                ObjectDigest::new([0; 32]),
            ),
            sender: self.address,
            recipient: Address::FastPay(recipient),
            gas_payment: (
                gas_payment,
                self.next_sequence_number(&gas_payment)?,
                // TODO(https://github.com/MystenLabs/fastnft/issues/123): Include actual object digest here
                ObjectDigest::new([0; 32]),
            ),
        };
        let order = Order::new_transfer(transfer, &self.secret);
        let new_certificate = self
            .execute_transfer(order, /* with_confirmation */ false)
            .await?;
        Ok(new_certificate)
    }

    async fn sync_client_state_with_random_authority(
        &mut self,
    ) -> Result<AuthorityName, anyhow::Error> {
        if let Some(order) = self.pending_transfer.clone() {
            // Finish executing the previous transfer.
            self.execute_transfer(order, /* with_confirmation */ false)
                .await?;
        }
        // update object_ids.
        self.object_ids.clear();

        let (authority_name, object_ids) = self.download_own_object_ids().await?;
        for (object_id, sequence_number, _) in object_ids {
            self.object_ids.insert(object_id, sequence_number);
        }
        // Recover missing certificates.
        let new_certificates = self.download_certificates().await?;

        for (id, certs) in new_certificates {
            self.update_certificates(&id, &certs)?;
        }
        Ok(authority_name)
    }

    async fn get_owned_objects(&self) -> Result<Vec<ObjectID>, anyhow::Error> {
        Ok(self.object_ids.keys().copied().collect())
    }

    async fn move_call(
        &mut self,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_object_ref: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
    ) -> Result<(CertifiedOrder, OrderEffects), anyhow::Error> {
        self.call(
            package_object_ref,
            module,
            function,
            type_arguments,
            gas_object_ref,
            object_arguments,
            pure_arguments,
            gas_budget,
        )
        .await
    }

    async fn publish(
        &mut self,
        package_source_files_path: String,
        gas_object_ref: ObjectRef,
    ) -> Result<(CertifiedOrder, OrderEffects), FastPayError> {
        self.publish(package_source_files_path, gas_object_ref)
            .await
    }

    async fn get_object_info(
        &mut self,
        object_info_req: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, anyhow::Error> {
        self.get_object_info_execute(object_info_req).await
    }
}
