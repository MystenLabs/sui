// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::downloader::*;
use anyhow::{bail, ensure};
use fastx_types::{
    base_types::*, committee::Committee, error::FastPayError, fp_ensure, messages::*,
};
use futures::stream::FuturesUnordered;
use futures::{future, StreamExt};
use rand::seq::SliceRandom;
use std::collections::{btree_map, BTreeMap, BTreeSet, HashMap};

#[cfg(test)]
#[path = "unit_tests/client_tests.rs"]
mod client_tests;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub trait AuthorityClient {
    /// Initiate a new order to a FastPay or Primary account.
    fn handle_order(&mut self, order: Order) -> AsyncResult<'_, AccountInfoResponse, FastPayError>;

    /// Confirm an order to a FastPay or Primary account.
    fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> AsyncResult<'_, AccountInfoResponse, FastPayError>;

    /// Handle information requests for this account.
    fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> AsyncResult<'_, AccountInfoResponse, FastPayError>;
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
    /// Expected sequence number for the next certified transfer.
    /// This is also the number of transfer certificates that we have created.
    next_sequence_number: SequenceNumber,
    /// Pending transfer.
    pending_transfer: Option<Order>,

    // The remaining fields are used to minimize networking, and may not always be persisted locally.
    /// Transfer certificates that we have created ("sent").
    /// Normally, `sent_certificates` should contain one certificate for each index in `0..next_sequence_number`.
    sent_certificates: Vec<CertifiedOrder>,
    /// Known received certificates, indexed by sender and sequence number.
    /// TODO: API to search and download yet unknown `received_certificates`.
    received_certificates: BTreeMap<(FastPayAddress, SequenceNumber), CertifiedOrder>,
    /// The known objects owned by the client.
    object_ids: Vec<ObjectID>,
}

// Operations are considered successful when they successfully reach a quorum of authorities.
pub trait Client {
    // TODO: refactor client interface to handle generic fastnft objects rather than payments / value transfers.

    /// Send money to a FastPay account.
    fn transfer_to_fastpay(
        &mut self,
        object_id: ObjectID,
        recipient: FastPayAddress,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error>;

    /// Send money to a Primary account.
    fn transfer_to_primary(
        &mut self,
        object_id: ObjectID,
        recipient: PrimaryAddress,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error>;

    /// Receive money from FastPay.
    fn receive_from_fastpay(
        &mut self,
        certificate: CertifiedOrder,
    ) -> AsyncResult<'_, (), anyhow::Error>;

    /// Send money to a FastPay account.
    /// Do not check balance. (This may block the client)
    /// Do not confirm the transaction.
    fn transfer_to_fastpay_unsafe_unconfirmed(
        &mut self,
        recipient: FastPayAddress,
        object_id: ObjectID,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error>;

    /// Find how much money we can spend.
    /// TODO: Currently, this value only reflects received transfers that were
    /// locally processed by `receive_from_fastpay`.
    fn get_spendable_amount(&mut self) -> AsyncResult<'_, Amount, anyhow::Error>;
}

impl<A> ClientState<A> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        address: FastPayAddress,
        secret: KeyPair,
        committee: Committee,
        authority_clients: HashMap<AuthorityName, A>,
        next_sequence_number: SequenceNumber,
        sent_certificates: Vec<CertifiedOrder>,
        received_certificates: Vec<CertifiedOrder>,
        object_ids: Vec<ObjectID>,
    ) -> Self {
        Self {
            address,
            secret,
            committee,
            authority_clients,
            next_sequence_number,
            pending_transfer: None,
            sent_certificates,
            received_certificates: received_certificates
                .into_iter()
                .map(|cert| (cert.key(), cert))
                .collect(),
            object_ids,
        }
    }

    pub fn address(&self) -> FastPayAddress {
        self.address
    }

    pub fn next_sequence_number(&self) -> SequenceNumber {
        self.next_sequence_number
    }

    pub fn object_ids(&self) -> &Vec<ObjectID> {
        &self.object_ids
    }

    pub fn pending_transfer(&self) -> &Option<Order> {
        &self.pending_transfer
    }

    pub fn sent_certificates(&self) -> &Vec<CertifiedOrder> {
        &self.sent_certificates
    }

    pub fn received_certificates(&self) -> impl Iterator<Item = &CertifiedOrder> {
        self.received_certificates.values()
    }
}

#[derive(Clone)]
struct CertificateRequester<A> {
    committee: Committee,
    authority_clients: Vec<A>,
    sender: FastPayAddress,
    object_id: ObjectID,
}

impl<A> CertificateRequester<A> {
    fn new(
        committee: Committee,
        authority_clients: Vec<A>,
        sender: FastPayAddress,
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

impl<A> Requester for CertificateRequester<A>
where
    A: AuthorityClient + Send + Sync + 'static + Clone,
{
    type Key = SequenceNumber;
    type Value = Result<CertifiedOrder, FastPayError>;

    /// Try to find a certificate for the given sender and sequence number.
    fn query(
        &mut self,
        sequence_number: SequenceNumber,
    ) -> AsyncResult<'_, CertifiedOrder, FastPayError> {
        Box::pin(async move {
            let request = AccountInfoRequest {
                object_id: self.object_id,
                request_sequence_number: Some(sequence_number),
                request_received_transfers_excluding_first_nth: None,
            };
            // Sequentially try each authority in random order.
            self.authority_clients.shuffle(&mut rand::thread_rng());
            for client in self.authority_clients.iter_mut() {
                let result = client.handle_account_info_request(request.clone()).await;
                if let Ok(AccountInfoResponse {
                    requested_certificate: Some(certificate),
                    ..
                }) = &result
                {
                    if certificate.check(&self.committee).is_ok() {
                        let order = &certificate.order;
                        if order.sender() == &self.sender
                            && order.sequence_number() == sequence_number
                        {
                            return Ok(certificate.clone());
                        }
                    }
                }
            }
            Err(FastPayError::ErrorWhileRequestingCertificate)
        })
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
            sender,
            object_id,
        )
        .query(sequence_number)
        .await
    }

    /// Find the highest sequence number that is known to a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_strong_majority_sequence_number(&self, object_id: ObjectID) -> SequenceNumber {
        let request = AccountInfoRequest {
            object_id,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        };
        let mut authority_clients = self.authority_clients.clone();
        let numbers: FuturesUnordered<_> = authority_clients
            .iter_mut()
            .map(|(name, client)| {
                let fut = client.handle_account_info_request(request.clone());
                async move {
                    match fut.await {
                        Ok(info) => Some((*name, info.next_sequence_number)),
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
        let request = AccountInfoRequest {
            object_id,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        };
        let authority_clients = self.authority_clients.clone();
        let numbers: futures::stream::FuturesUnordered<_> = authority_clients
            .iter()
            .map(|(name, client)| {
                let fut = client.handle_account_info_request(request.clone());
                async move {
                    match fut.await {
                        Ok(info) => Some((*name, Some((info.owner, info.next_sequence_number)))),
                        _ => None,
                    }
                }
            })
            .collect();
        self.committee
            .get_strong_majority_lower_bound(
                numbers.filter_map(|x| async move { x }).collect().await,
            )
            .is_some()
    }

    /// Execute a sequence of actions in parallel for a quorum of authorities.
    async fn communicate_with_quorum<'a, V, F>(
        &'a mut self,
        execute: F,
    ) -> Result<Vec<V>, anyhow::Error>
    where
        F: Fn(AuthorityName, &'a mut A) -> AsyncResult<'a, V, FastPayError> + Clone,
    {
        let committee = &self.committee;
        let authority_clients = &mut self.authority_clients;
        let mut responses: FuturesUnordered<_> = authority_clients
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
                        bail!(
                            "Failed to communicate with a quorum of authorities: {}",
                            err
                        );
                    }
                }
            }
        }

        bail!("Failed to communicate with a quorum of authorities (multiple errors)");
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
            sender,
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
                    let request = AccountInfoRequest {
                        object_id,
                        request_sequence_number: None,
                        request_received_transfers_excluding_first_nth: None,
                    };
                    let response = client.handle_account_info_request(request).await?;
                    let current_sequence_number = response.next_sequence_number;
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
                        let result = client.handle_order(order).await;
                        match result {
                            Ok(AccountInfoResponse {
                                pending_confirmation: Some(signed_order),
                                ..
                            }) => {
                                fp_ensure!(
                                    signed_order.authority == name,
                                    FastPayError::ErrorWhileProcessingTransferOrder
                                );
                                signed_order.check(committee)?;
                                return Ok(Some(signed_order));
                            }
                            Err(err) => return Err(err),
                            _ => return Err(FastPayError::ErrorWhileProcessingTransferOrder),
                        }
                    }
                    Ok(None)
                })
            })
            .await?;
        // Terminate downloader task and retrieve the content of the cache.
        handle.stop().await?;
        let mut certificates: Vec<_> = task.await.unwrap().filter_map(Result::ok).collect();
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
    async fn download_sent_certificates(&self) -> Result<Vec<CertifiedOrder>, FastPayError> {
        let mut requesters: Vec<CertificateRequester<_>> = self
            .object_ids
            .iter()
            .map(|object_id| {
                CertificateRequester::new(
                    self.committee.clone(),
                    self.authority_clients.values().cloned().collect(),
                    self.address,
                    *object_id,
                )
            })
            .collect();
        let known_sequence_numbers: BTreeSet<_> = self
            .sent_certificates
            .iter()
            .map(|cert| cert.order.sequence_number())
            .collect();
        let mut sent_certificates = self.sent_certificates.clone();
        let mut number = SequenceNumber::from(0);
        while number < self.next_sequence_number {
            if !known_sequence_numbers.contains(&number) {
                let mut requesters = requesters.iter_mut();
                // Because we don't know which object is associated to which sequence number, so we try to query each authority with the sequence number until we have a hit.
                // TODO: Rethink how we store sequence number, maybe the client should hold a map of sequence number to object id, or each object should have their own sequence number.
                let certificate = loop {
                    let requester = requesters
                        .next()
                        .ok_or(FastPayError::CertificateNotfound)
                        .unwrap();
                    if let Ok(cert) = requester.query(number).await {
                        break cert;
                    }
                };
                sent_certificates.push(certificate);
            }
            number = number.increment().unwrap_or_else(|_| SequenceNumber::max());
        }
        sent_certificates.sort_by_key(|cert| cert.order.sequence_number());
        Ok(sent_certificates)
    }

    /// Transfers an object to a recipient address.
    async fn transfer(
        &mut self,
        object_id: ObjectID,
        recipient: Address,
        user_data: UserData,
    ) -> Result<CertifiedOrder, anyhow::Error> {
        let transfer = Transfer {
            object_id,
            sender: self.address,
            recipient,
            sequence_number: self.next_sequence_number,
            user_data,
        };
        let order = Order::new_transfer(transfer, &self.secret);
        let certificate = self
            .execute_transfer(order, /* with_confirmation */ true)
            .await?;
        Ok(certificate)
    }

    /// Update our view of sent certificates. Adjust the local balance and the next sequence number accordingly.
    /// NOTE: This is only useful in the eventuality of missing local data.
    /// We assume certificates to be valid and sent by us, and their sequence numbers to be unique.
    fn update_sent_certificates(
        &mut self,
        sent_certificates: Vec<CertifiedOrder>,
    ) -> Result<(), FastPayError> {
        let mut new_next_sequence_number = self.next_sequence_number;
        for new_cert in &sent_certificates {
            if new_cert.order.sequence_number() >= new_next_sequence_number {
                new_next_sequence_number = new_cert
                    .order
                    .sequence_number()
                    .increment()
                    .unwrap_or_else(|_| SequenceNumber::max());
            }
        }
        /*

        */
        // Atomic update
        self.sent_certificates = sent_certificates;
        self.next_sequence_number = new_next_sequence_number;
        // Sanity check
        assert_eq!(
            self.sent_certificates.len(),
            usize::from(self.next_sequence_number)
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
            order.sequence_number() == self.next_sequence_number,
            "Unexpected sequence number"
        );
        self.pending_transfer = Some(order.clone());
        let new_sent_certificates = self
            .communicate_transfers(
                self.address,
                *order.object_id(),
                self.sent_certificates.clone(),
                CommunicateAction::SendOrder(order.clone()),
            )
            .await?;
        assert_eq!(new_sent_certificates.last().unwrap().order, order);
        // Clear `pending_transfer` and update `sent_certificates`,
        // and `next_sequence_number`. (Note that if we were using persistent
        // storage, we should ensure update atomicity in the eventuality of a crash.)
        self.pending_transfer = None;
        self.update_sent_certificates(new_sent_certificates)?;
        // Confirm last transfer certificate if needed.
        if with_confirmation {
            self.communicate_transfers(
                self.address,
                *order.object_id(),
                self.sent_certificates.clone(),
                CommunicateAction::SynchronizeNextSequenceNumber(self.next_sequence_number),
            )
            .await?;
        }
        Ok(self.sent_certificates.last().unwrap().clone())
    }
}

impl<A> Client for ClientState<A>
where
    A: AuthorityClient + Send + Sync + Clone + 'static,
{
    fn transfer_to_fastpay(
        &mut self,
        object_id: ObjectID,
        recipient: FastPayAddress,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error> {
        Box::pin(self.transfer(object_id, Address::FastPay(recipient), user_data))
    }

    fn transfer_to_primary(
        &mut self,
        object_id: ObjectID,
        recipient: PrimaryAddress,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error> {
        Box::pin(self.transfer(object_id, Address::Primary(recipient), user_data))
    }

    fn get_spendable_amount(&mut self) -> AsyncResult<'_, Amount, anyhow::Error> {
        Box::pin(async move {
            if let Some(order) = self.pending_transfer.clone() {
                // Finish executing the previous transfer.
                self.execute_transfer(order, /* with_confirmation */ false)
                    .await?;
            }
            if self.sent_certificates.len() < self.next_sequence_number.into() {
                // Recover missing sent certificates.
                let new_sent_certificates = self.download_sent_certificates().await?;
                self.update_sent_certificates(new_sent_certificates)?;
            }
            /*            let amount = if self.balance < Balance::zero() {
                Amount::zero()
            } else {
                Amount::try_from(self.balance).unwrap_or_else(|_| std::u64::MAX.into())
            };*/
            Ok(Amount::zero())
        })
    }

    fn receive_from_fastpay(
        &mut self,
        certificate: CertifiedOrder,
    ) -> AsyncResult<'_, (), anyhow::Error> {
        Box::pin(async move {
            certificate.check(&self.committee)?;
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
                            transfer.sequence_number.increment()?,
                        ),
                    )
                    .await?;
                    // Everything worked: update the local balance.
                    if let btree_map::Entry::Vacant(entry) =
                        self.received_certificates.entry(transfer.key())
                    {
                        // self.balance = self.balance.try_add(transfer.amount.into())?;
                        entry.insert(certificate);
                    }
                    Ok(())
                }
                OrderKind::Publish(_) | OrderKind::Call(_) => {
                    unimplemented!("receiving (?) Move call or publish")
                }
            }
        })
    }

    fn transfer_to_fastpay_unsafe_unconfirmed(
        &mut self,
        recipient: FastPayAddress,
        object_id: ObjectID,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error> {
        Box::pin(async move {
            let transfer = Transfer {
                object_id,
                sender: self.address,
                recipient: Address::FastPay(recipient),
                // amount,
                sequence_number: self.next_sequence_number,
                user_data,
            };
            let order = Order::new_transfer(transfer, &self.secret);
            let new_certificate = self
                .execute_transfer(order, /* with_confirmation */ false)
                .await?;
            Ok(new_certificate)
        })
    }
}
