// Copyright (c) Facebook Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::*;
use crate::committee::Committee;
use crate::error::FastPayError;
use crate::messages::*;
use std::collections::BTreeMap;
use std::convert::TryInto;

#[cfg(test)]
#[path = "unit_tests/authority_tests.rs"]
mod authority_tests;

#[derive(Eq, PartialEq, Debug)]
pub struct AccountOffchainState {
    /// Balance of the FastPay account.
    pub balance: Balance,
    /// Sequence number tracking spending actions.
    pub next_sequence_number: SequenceNumber,
    /// Whether we have signed a transfer for this sequence number already.
    pub pending_confirmation: Option<SignedTransferOrder>,
    /// All confirmed certificates for this sender.
    pub confirmed_log: Vec<CertifiedTransferOrder>,
    /// All executed Primary synchronization orders for this recipient.
    pub synchronization_log: Vec<PrimarySynchronizationOrder>,
    /// All confirmed certificates as a receiver.
    pub received_log: Vec<CertifiedTransferOrder>,
}

pub struct AuthorityState {
    /// The name of this autority.
    pub name: AuthorityName,
    /// Committee of this FastPay instance.
    pub committee: Committee,
    /// The signature key of the authority.
    pub secret: SecretKey,
    /// Offchain states of FastPay accounts.
    pub accounts: BTreeMap<FastPayAddress, AccountOffchainState>,
    /// The latest transaction index of the blockchain that the authority has seen.
    pub last_transaction_index: VersionNumber,
    /// The sharding ID of this authority shard. 0 if one shard.
    pub shard_id: ShardId,
    /// The number of shards. 1 if single shard.
    pub number_of_shards: u32,
}

/// Interface provided by each (shard of an) authority.
/// All commands return either the current account info or an error.
/// Repeating commands produces no changes and returns no error.
pub trait Authority {
    /// Initiate a new transfer to a FastPay or Primary account.
    fn handle_transfer_order(
        &mut self,
        order: TransferOrder,
    ) -> Result<AccountInfoResponse, FastPayError>;

    /// Confirm a transfer to a FastPay or Primary account.
    fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> Result<(AccountInfoResponse, Option<CrossShardUpdate>), FastPayError>;

    /// Force synchronization to finalize transfers from Primary to FastPay.
    fn handle_primary_synchronization_order(
        &mut self,
        order: PrimarySynchronizationOrder,
    ) -> Result<AccountInfoResponse, FastPayError>;

    /// Handle information requests for this account.
    fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError>;

    /// Handle cross updates from another shard of the same authority.
    /// This relies on deliver-once semantics of a trusted channel between shards.
    fn handle_cross_shard_recipient_commit(
        &mut self,
        certificate: CertifiedTransferOrder,
    ) -> Result<(), FastPayError>;
}

impl Authority for AuthorityState {
    /// Initiate a new transfer.
    fn handle_transfer_order(
        &mut self,
        order: TransferOrder,
    ) -> Result<AccountInfoResponse, FastPayError> {
        // Check the sender's signature and retrieve the transfer data.
        fp_ensure!(
            self.in_shard(&order.transfer.sender),
            FastPayError::WrongShard
        );
        order.check_signature()?;
        let transfer = &order.transfer;
        let sender = transfer.sender;
        fp_ensure!(
            transfer.sequence_number <= SequenceNumber::max(),
            FastPayError::InvalidSequenceNumber
        );
        fp_ensure!(
            transfer.amount > Amount::zero(),
            FastPayError::IncorrectTransferAmount
        );
        match self.accounts.get_mut(&sender) {
            None => fp_bail!(FastPayError::UnknownSenderAccount),
            Some(account) => {
                if let Some(pending_confirmation) = &account.pending_confirmation {
                    fp_ensure!(
                        &pending_confirmation.value.transfer == transfer,
                        FastPayError::PreviousTransferMustBeConfirmedFirst {
                            pending_confirmation: pending_confirmation.value.clone()
                        }
                    );
                    // This exact transfer order was already signed. Return the previous value.
                    return Ok(account.make_account_info(sender));
                }
                fp_ensure!(
                    account.next_sequence_number == transfer.sequence_number,
                    FastPayError::UnexpectedSequenceNumber
                );
                fp_ensure!(
                    account.balance >= transfer.amount.into(),
                    FastPayError::InsufficientFunding {
                        current_balance: account.balance
                    }
                );
                let signed_order = SignedTransferOrder::new(order, self.name, &self.secret);
                account.pending_confirmation = Some(signed_order.clone());
                Ok(account.make_account_info(sender))
            }
        }
    }

    /// Confirm a transfer.
    fn handle_confirmation_order(
        &mut self,
        confirmation_order: ConfirmationOrder,
    ) -> Result<(AccountInfoResponse, Option<CrossShardUpdate>), FastPayError> {
        let certificate = confirmation_order.transfer_certificate;
        // Check the certificate and retrieve the transfer data.
        fp_ensure!(
            self.in_shard(&certificate.value.transfer.sender),
            FastPayError::WrongShard
        );
        certificate.check(&self.committee)?;
        let transfer = certificate.value.transfer.clone();

        // First we copy all relevant data from sender.
        let mut sender_account = self
            .accounts
            .entry(transfer.sender)
            .or_insert(AccountOffchainState::new());
        let mut sender_sequence_number = sender_account.next_sequence_number;
        let mut sender_balance = sender_account.balance;

        // Check and update the copied state
        if sender_sequence_number < transfer.sequence_number {
            fp_bail!(FastPayError::MissingEalierConfirmations {
                current_sequence_number: sender_sequence_number
            });
        }
        if sender_sequence_number > transfer.sequence_number {
            // Transfer was already confirmed.
            return Ok((sender_account.make_account_info(transfer.sender), None));
        }
        sender_balance = sender_balance.sub(transfer.amount.into())?;
        sender_sequence_number = sender_sequence_number.increment()?;

        // Commit sender state back to the database (Must never fail!)
        sender_account.balance = sender_balance;
        sender_account.next_sequence_number = sender_sequence_number;
        sender_account.pending_confirmation = None;
        sender_account.confirmed_log.push(certificate.clone());
        let info = sender_account.make_account_info(transfer.sender);

        // Update FastPay recipient state locally or issue a cross-shard update (Must never fail!)
        let recipient = match transfer.recipient {
            Address::FastPay(recipient) => recipient,
            Address::Primary(_) => {
                // Nothing else to do for Primary recipients.
                return Ok((info, None));
            }
        };
        // If the recipient is in the same shard, read and update the account.
        if self.in_shard(&recipient) {
            let recipient_account = self
                .accounts
                .entry(recipient)
                .or_insert(AccountOffchainState::new());
            recipient_account.balance = recipient_account
                .balance
                .add(transfer.amount.into())
                .unwrap_or(Balance::max());
            recipient_account.received_log.push(certificate.clone());
            // Done updating recipient.
            return Ok((info, None));
        }
        // Otherwise, we need to send a cross-shard update.
        let cross_shard = Some(CrossShardUpdate {
            shard_id: self.which_shard(&recipient),
            transfer_certificate: certificate,
        });
        Ok((info, cross_shard))
    }

    // NOTE: Need to rely on deliver-once semantics from comms channel
    fn handle_cross_shard_recipient_commit(
        &mut self,
        certificate: CertifiedTransferOrder,
    ) -> Result<(), FastPayError> {
        // TODO: check certificate again?
        let transfer = &certificate.value.transfer;

        let recipient = match transfer.recipient {
            Address::FastPay(recipient) => recipient,
            Address::Primary(_) => {
                fp_bail!(FastPayError::InvalidCrossShardUpdate);
            }
        };
        fp_ensure!(self.in_shard(&recipient), FastPayError::WrongShard);
        let recipient_account = self
            .accounts
            .entry(recipient)
            .or_insert(AccountOffchainState::new());
        recipient_account.balance = recipient_account
            .balance
            .add(transfer.amount.into())
            .unwrap_or(Balance::max());
        recipient_account.received_log.push(certificate);
        Ok(())
    }

    /// Finalize a transfer from Primary.
    fn handle_primary_synchronization_order(
        &mut self,
        order: PrimarySynchronizationOrder,
    ) -> Result<AccountInfoResponse, FastPayError> {
        // Update recipient state; note that the blockchain client is trusted.
        let recipient = order.recipient;
        fp_ensure!(self.in_shard(&recipient), FastPayError::WrongShard);

        let recipient_account = self
            .accounts
            .entry(recipient)
            .or_insert(AccountOffchainState::new());
        if order.transaction_index <= self.last_transaction_index {
            // Ignore old transaction index.
            return Ok(recipient_account.make_account_info(recipient));
        }
        fp_ensure!(
            order.transaction_index == self.last_transaction_index.increment()?,
            FastPayError::UnexpectedTransactionIndex
        );
        let recipient_balance = recipient_account.balance.add(order.amount.into())?;
        let last_transaction_index = self.last_transaction_index.increment()?;
        recipient_account.balance = recipient_balance;
        recipient_account.synchronization_log.push(order);
        self.last_transaction_index = last_transaction_index;
        Ok(recipient_account.make_account_info(recipient))
    }

    fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError> {
        fp_ensure!(self.in_shard(&request.sender), FastPayError::WrongShard);
        let account = self.account_state(&request.sender)?;
        let mut response = account.make_account_info(request.sender);
        if let Some(seq) = request.request_sequence_number {
            if let Some(cert) = account.confirmed_log.get(usize::from(seq)) {
                response.requested_certificate = Some(cert.clone());
            } else {
                fp_bail!(FastPayError::CertificateNotfound)
            }
        }
        if let Some(idx) = request.request_received_transfers_excluding_first_nth {
            response.requested_received_transfers = account.received_log[idx..].to_vec();
        }
        Ok(response)
    }
}

impl AccountOffchainState {
    pub fn new() -> Self {
        Self {
            balance: Balance::zero(),
            next_sequence_number: SequenceNumber::new(),
            pending_confirmation: None,
            confirmed_log: Vec::new(),
            synchronization_log: Vec::new(),
            received_log: Vec::new(),
        }
    }

    fn make_account_info(&self, sender: FastPayAddress) -> AccountInfoResponse {
        AccountInfoResponse {
            sender,
            balance: self.balance,
            next_sequence_number: self.next_sequence_number,
            pending_confirmation: self.pending_confirmation.clone(),
            requested_certificate: None,
            requested_received_transfers: Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn new_with_balance(balance: Balance, received_log: Vec<CertifiedTransferOrder>) -> Self {
        Self {
            balance,
            next_sequence_number: SequenceNumber::new(),
            pending_confirmation: None,
            confirmed_log: Vec::new(),
            synchronization_log: Vec::new(),
            received_log,
        }
    }
}

impl AuthorityState {
    pub fn new(committee: Committee, name: AuthorityName, secret: SecretKey) -> Self {
        AuthorityState {
            committee,
            name,
            secret,
            accounts: BTreeMap::new(),
            last_transaction_index: VersionNumber::new(),
            shard_id: 0,
            number_of_shards: 1,
        }
    }

    pub fn new_shard(
        committee: Committee,
        name: AuthorityName,
        secret: SecretKey,
        shard_id: u32,
        number_of_shards: u32,
    ) -> Self {
        AuthorityState {
            committee,
            name,
            secret,
            accounts: BTreeMap::new(),
            last_transaction_index: VersionNumber::new(),
            shard_id,
            number_of_shards,
        }
    }

    pub fn in_shard(&self, address: &FastPayAddress) -> bool {
        self.which_shard(address) == self.shard_id
    }

    pub fn get_shard(num_shards: u32, address: &FastPayAddress) -> u32 {
        const LAST_INTEGER_INDEX: usize = std::mem::size_of::<FastPayAddress>() - 4;
        u32::from_le_bytes(address.0[LAST_INTEGER_INDEX..].try_into().expect("4 bytes"))
            % num_shards
    }

    pub fn which_shard(&self, address: &FastPayAddress) -> u32 {
        Self::get_shard(self.number_of_shards, address)
    }

    fn account_state(
        &self,
        address: &FastPayAddress,
    ) -> Result<&AccountOffchainState, FastPayError> {
        self.accounts
            .get(address)
            .ok_or_else(|| FastPayError::UnknownSenderAccount)
    }

    #[cfg(test)]
    pub fn accounts_mut(&mut self) -> &mut BTreeMap<FastPayAddress, AccountOffchainState> {
        &mut self.accounts
    }
}
