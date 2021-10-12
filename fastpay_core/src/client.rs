// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::{base_types::*, committee::Committee, downloader::*, error::FastPayError, messages::*};
use failure::{bail, ensure};
use futures::{future, StreamExt};
use rand::seq::SliceRandom;
use std::{
    collections::{btree_map, BTreeMap, BTreeSet, HashMap},
    convert::TryFrom,
};

#[cfg(test)]
#[path = "unit_tests/client_tests.rs"]
mod client_tests;

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub trait AuthorityClient {
    /// Initiate a new transfer to a FastPay or Primary account.
    fn handle_transfer_order(
        &mut self,
        order: TransferOrder,
    ) -> AsyncResult<AccountInfoResponse, FastPayError>;

    /// Confirm a transfer to a FastPay or Primary account.
    fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> AsyncResult<AccountInfoResponse, FastPayError>;

    /// Handle information requests for this account.
    fn handle_account_info_request(
        &mut self,
        request: AccountInfoRequest,
    ) -> AsyncResult<AccountInfoResponse, FastPayError>;
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
    pending_transfer: Option<TransferOrder>,

    // The remaining fields are used to minimize networking, and may not always be persisted locally.
    /// Transfer certificates that we have created ("sent").
    /// Normally, `sent_certificates` should contain one certificate for each index in `0..next_sequence_number`.
    sent_certificates: Vec<CertifiedTransferOrder>,
    /// Known received certificates, indexed by sender and sequence number.
    /// TODO: API to search and download yet unknown `received_certificates`.
    received_certificates: BTreeMap<(FastPayAddress, SequenceNumber), CertifiedTransferOrder>,
    /// The known spendable balance (including a possible initial funding, excluding unknown sent
    /// or received certificates).
    balance: Balance,
}

// Operations are considered successful when they successfully reach a quorum of authorities.
pub trait Client {
    /// Send money to a FastPay account.
    fn transfer_to_fastpay(
        &mut self,
        amount: Amount,
        recipient: FastPayAddress,
        user_data: UserData,
    ) -> AsyncResult<CertifiedTransferOrder, failure::Error>;

    /// Send money to a Primary account.
    fn transfer_to_primary(
        &mut self,
        amount: Amount,
        recipient: PrimaryAddress,
        user_data: UserData,
    ) -> AsyncResult<CertifiedTransferOrder, failure::Error>;

    /// Receive money from FastPay.
    fn receive_from_fastpay(
        &mut self,
        certificate: CertifiedTransferOrder,
    ) -> AsyncResult<(), failure::Error>;

    /// Send money to a FastPay account.
    /// Do not check balance. (This may block the client)
    /// Do not confirm the transaction.
    fn transfer_to_fastpay_unsafe_unconfirmed(
        &mut self,
        amount: Amount,
        recipient: FastPayAddress,
        user_data: UserData,
    ) -> AsyncResult<CertifiedTransferOrder, failure::Error>;

    /// Find how much money we can spend.
    /// TODO: Currently, this value only reflects received transfers that were
    /// locally processed by `receive_from_fastpay`.
    fn get_spendable_amount(&mut self) -> AsyncResult<Amount, failure::Error>;
}

impl<A> ClientState<A> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        address: FastPayAddress,
        secret: KeyPair,
        committee: Committee,
        authority_clients: HashMap<AuthorityName, A>,
        next_sequence_number: SequenceNumber,
        sent_certificates: Vec<CertifiedTransferOrder>,
        received_certificates: Vec<CertifiedTransferOrder>,
        balance: Balance,
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
            balance,
        }
    }

    pub fn address(&self) -> FastPayAddress {
        self.address
    }

    pub fn next_sequence_number(&self) -> SequenceNumber {
        self.next_sequence_number
    }

    pub fn balance(&self) -> Balance {
        self.balance
    }

    pub fn pending_transfer(&self) -> &Option<TransferOrder> {
        &self.pending_transfer
    }

    pub fn sent_certificates(&self) -> &Vec<CertifiedTransferOrder> {
        &self.sent_certificates
    }

    pub fn received_certificates(&self) -> impl Iterator<Item = &CertifiedTransferOrder> {
        self.received_certificates.values()
    }
}

#[derive(Clone)]
struct CertificateRequester<A> {
    committee: Committee,
    authority_clients: Vec<A>,
    sender: FastPayAddress,
}

impl<A> CertificateRequester<A> {
    fn new(committee: Committee, authority_clients: Vec<A>, sender: FastPayAddress) -> Self {
        Self {
            committee,
            authority_clients,
            sender,
        }
    }
}

impl<A> Requester for CertificateRequester<A>
where
    A: AuthorityClient + Send + Sync + 'static + Clone,
{
    type Key = SequenceNumber;
    type Value = Result<CertifiedTransferOrder, FastPayError>;

    /// Try to find a certificate for the given sender and sequence number.
    fn query(
        &mut self,
        sequence_number: SequenceNumber,
    ) -> AsyncResult<CertifiedTransferOrder, FastPayError> {
        Box::pin(async move {
            let request = AccountInfoRequest {
                sender: self.sender,
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
                        let transfer = &certificate.value.transfer;
                        if transfer.sender == self.sender
                            && transfer.sequence_number == sequence_number
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
enum CommunicateAction {
    SendOrder(TransferOrder),
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
        sequence_number: SequenceNumber,
    ) -> Result<CertifiedTransferOrder, FastPayError> {
        CertificateRequester::new(
            self.committee.clone(),
            self.authority_clients.values().cloned().collect(),
            sender,
        )
        .query(sequence_number)
        .await
    }

    /// Find the highest sequence number that is known to a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_strong_majority_sequence_number(
        &mut self,
        sender: FastPayAddress,
    ) -> SequenceNumber {
        let request = AccountInfoRequest {
            sender,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        };
        let numbers: futures::stream::FuturesUnordered<_> = self
            .authority_clients
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

    /// Find the highest balance that is backed by a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_strong_majority_balance(&mut self) -> Balance {
        let request = AccountInfoRequest {
            sender: self.address,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        };
        let numbers: futures::stream::FuturesUnordered<_> = self
            .authority_clients
            .iter_mut()
            .map(|(name, client)| {
                let fut = client.handle_account_info_request(request.clone());
                async move {
                    match fut.await {
                        Ok(info) => Some((*name, info.balance)),
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
    ) -> Result<Vec<V>, failure::Error>
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
        known_certificates: Vec<CertifiedTransferOrder>,
        action: CommunicateAction,
    ) -> Result<Vec<CertifiedTransferOrder>, failure::Error> {
        let target_sequence_number = match &action {
            CommunicateAction::SendOrder(order) => order.transfer.sequence_number,
            CommunicateAction::SynchronizeNextSequenceNumber(seq) => *seq,
        };
        let requester = CertificateRequester::new(
            self.committee.clone(),
            self.authority_clients.values().cloned().collect(),
            sender,
        );
        let (task, mut handle) = Downloader::start(
            requester,
            known_certificates.into_iter().filter_map(|cert| {
                if cert.value.transfer.sender == sender {
                    Some((cert.value.transfer.sequence_number, Ok(cert)))
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
                        sender,
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
                        let result = client.handle_transfer_order(order).await;
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
            let certificate = CertifiedTransferOrder {
                value: order,
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
    async fn download_sent_certificates(
        &self,
    ) -> Result<Vec<CertifiedTransferOrder>, FastPayError> {
        let mut requester = CertificateRequester::new(
            self.committee.clone(),
            self.authority_clients.values().cloned().collect(),
            self.address,
        );
        let known_sequence_numbers: BTreeSet<_> = self
            .sent_certificates
            .iter()
            .map(|cert| cert.value.transfer.sequence_number)
            .collect();
        let mut sent_certificates = self.sent_certificates.clone();
        let mut number = SequenceNumber::from(0);
        while number < self.next_sequence_number {
            if !known_sequence_numbers.contains(&number) {
                let certificate = requester.query(number).await?;
                sent_certificates.push(certificate);
            }
            number = number.increment().unwrap_or_else(|_| SequenceNumber::max());
        }
        sent_certificates.sort_by_key(|cert| cert.value.transfer.sequence_number);
        Ok(sent_certificates)
    }

    /// Send money to a FastPay or Primary recipient.
    async fn transfer(
        &mut self,
        amount: Amount,
        recipient: Address,
        user_data: UserData,
    ) -> Result<CertifiedTransferOrder, failure::Error> {
        // Trying to overspend may block the account. To prevent this, we compare with
        // the balance as we know it.
        let safe_amount = self.get_spendable_amount().await?;
        ensure!(
            amount <= safe_amount,
            "Requested amount ({:?}) is not backed by sufficient funds ({:?})",
            amount,
            safe_amount
        );
        let transfer = Transfer {
            sender: self.address,
            recipient,
            amount,
            sequence_number: self.next_sequence_number,
            user_data,
        };
        let order = TransferOrder::new(transfer, &self.secret);
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
        sent_certificates: Vec<CertifiedTransferOrder>,
    ) -> Result<(), FastPayError> {
        let mut new_balance = self.balance;
        let mut new_next_sequence_number = self.next_sequence_number;
        for new_cert in &sent_certificates {
            new_balance = new_balance.try_sub(new_cert.value.transfer.amount.into())?;
            if new_cert.value.transfer.sequence_number >= new_next_sequence_number {
                new_next_sequence_number = new_cert
                    .value
                    .transfer
                    .sequence_number
                    .increment()
                    .unwrap_or_else(|_| SequenceNumber::max());
            }
        }
        for old_cert in &self.sent_certificates {
            new_balance = new_balance.try_add(old_cert.value.transfer.amount.into())?;
        }
        // Atomic update
        self.sent_certificates = sent_certificates;
        self.balance = new_balance;
        self.next_sequence_number = new_next_sequence_number;
        // Sanity check
        assert_eq!(
            self.sent_certificates.len(),
            self.next_sequence_number.into()
        );
        Ok(())
    }

    /// Execute (or retry) a transfer order. Update local balance.
    async fn execute_transfer(
        &mut self,
        order: TransferOrder,
        with_confirmation: bool,
    ) -> Result<CertifiedTransferOrder, failure::Error> {
        ensure!(
            self.pending_transfer == None || self.pending_transfer.as_ref() == Some(&order),
            "Client state has a different pending transfer",
        );
        ensure!(
            order.transfer.sequence_number == self.next_sequence_number,
            "Unexpected sequence number"
        );
        self.pending_transfer = Some(order.clone());
        let new_sent_certificates = self
            .communicate_transfers(
                self.address,
                self.sent_certificates.clone(),
                CommunicateAction::SendOrder(order.clone()),
            )
            .await?;
        assert_eq!(new_sent_certificates.last().unwrap().value, order);
        // Clear `pending_transfer` and update `sent_certificates`,
        // `balance`, and `next_sequence_number`. (Note that if we were using persistent
        // storage, we should ensure update atomicity in the eventuality of a crash.)
        self.pending_transfer = None;
        self.update_sent_certificates(new_sent_certificates)?;
        // Confirm last transfer certificate if needed.
        if with_confirmation {
            self.communicate_transfers(
                self.address,
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
        amount: Amount,
        recipient: FastPayAddress,
        user_data: UserData,
    ) -> AsyncResult<CertifiedTransferOrder, failure::Error> {
        Box::pin(self.transfer(amount, Address::FastPay(recipient), user_data))
    }

    fn transfer_to_primary(
        &mut self,
        amount: Amount,
        recipient: PrimaryAddress,
        user_data: UserData,
    ) -> AsyncResult<CertifiedTransferOrder, failure::Error> {
        Box::pin(self.transfer(amount, Address::Primary(recipient), user_data))
    }

    fn get_spendable_amount(&mut self) -> AsyncResult<Amount, failure::Error> {
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
            let amount = if self.balance < Balance::zero() {
                Amount::zero()
            } else {
                Amount::try_from(self.balance).unwrap_or_else(|_| std::u64::MAX.into())
            };
            Ok(amount)
        })
    }

    fn receive_from_fastpay(
        &mut self,
        certificate: CertifiedTransferOrder,
    ) -> AsyncResult<(), failure::Error> {
        Box::pin(async move {
            certificate.check(&self.committee)?;
            let transfer = &certificate.value.transfer;
            ensure!(
                transfer.recipient == Address::FastPay(self.address),
                "Transfer should be received by us."
            );
            self.communicate_transfers(
                transfer.sender,
                vec![certificate.clone()],
                CommunicateAction::SynchronizeNextSequenceNumber(
                    certificate.value.transfer.sequence_number.increment()?,
                ),
            )
            .await?;
            // Everything worked: update the local balance.
            let transfer = &certificate.value.transfer;
            if let btree_map::Entry::Vacant(entry) =
                self.received_certificates.entry(transfer.key())
            {
                self.balance = self.balance.try_add(transfer.amount.into())?;
                entry.insert(certificate);
            }
            Ok(())
        })
    }

    fn transfer_to_fastpay_unsafe_unconfirmed(
        &mut self,
        amount: Amount,
        recipient: FastPayAddress,
        user_data: UserData,
    ) -> AsyncResult<CertifiedTransferOrder, failure::Error> {
        Box::pin(async move {
            let transfer = Transfer {
                sender: self.address,
                recipient: Address::FastPay(recipient),
                amount,
                sequence_number: self.next_sequence_number,
                user_data,
            };
            let order = TransferOrder::new(transfer, &self.secret);
            let new_certificate = self
                .execute_transfer(order, /* with_confirmation */ false)
                .await?;
            Ok(new_certificate)
        })
    }
}
