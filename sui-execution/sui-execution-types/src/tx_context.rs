// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use sui_protocol_config::ProtocolConfig;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::base_types::{ObjectID, SuiAddress, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME};
use sui_types::committee::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::epoch_data::EpochData;
use sui_types::error::{ExecutionError, ExecutionErrorKind};
use sui_types::messages_checkpoint::CheckpointTimestamp;

//
// `TxContext` in Rust (see below) is going to be purely used in Rust and can
// evolve as needed without worrying any compatibility with Move.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct MoveLegacyTxContext {
    // Signer/sender of the transaction
    sender: AccountAddress,
    // Digest of the current transaction
    digest: Vec<u8>,
    // The current epoch number
    epoch: EpochId,
    // Timestamp that the epoch started at
    epoch_timestamp_ms: CheckpointTimestamp,
    // Number of `ObjectID`'s generated during execution of the current transaction
    ids_created: u64,
}

impl From<&TxContext> for MoveLegacyTxContext {
    fn from(tx_context: &TxContext) -> Self {
        Self {
            sender: tx_context.sender,
            digest: tx_context.digest.clone(),
            epoch: tx_context.epoch,
            epoch_timestamp_ms: tx_context.epoch_timestamp_ms,
            ids_created: tx_context.ids_created,
        }
    }
}

// Information about the transaction context.
// This struct is not related to Move and can evolve as needed/required.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TxContext {
    /// Sender of the transaction
    sender: AccountAddress,
    /// Digest of the current transaction
    digest: Vec<u8>,
    /// The current epoch number
    epoch: EpochId,
    /// Timestamp that the epoch started at
    epoch_timestamp_ms: CheckpointTimestamp,
    /// Number of `ObjectID`'s generated during execution of the current transaction
    ids_created: u64,
    // Reference gas price
    rgp: u64,
    // gas price passed to transaction as input
    gas_price: u64,
    // gas budget passed to transaction as input
    gas_budget: u64,
    // address of the sponsor if any
    sponsor: Option<AccountAddress>,
    // whether the `TxContext` is native or not
    // (TODO: once we version execution we could drop this field)
    is_native: bool,
}

impl TxContext {
    pub fn new(
        sender: &SuiAddress,
        digest: &TransactionDigest,
        epoch_data: &EpochData,
        rgp: u64,
        gas_price: u64,
        gas_budget: u64,
        sponsor: Option<SuiAddress>,
        protocol_config: &ProtocolConfig,
    ) -> Self {
        Self::new_from_components(
            sender,
            digest,
            &epoch_data.epoch_id(),
            epoch_data.epoch_start_timestamp(),
            rgp,
            gas_price,
            gas_budget,
            sponsor,
            protocol_config,
        )
    }

    pub fn new_from_components(
        sender: &SuiAddress,
        digest: &TransactionDigest,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        rgp: u64,
        gas_price: u64,
        gas_budget: u64,
        sponsor: Option<SuiAddress>,
        protocol_config: &ProtocolConfig,
    ) -> Self {
        Self {
            sender: (*sender).into(),
            digest: digest.into_inner().to_vec(),
            epoch: *epoch_id,
            epoch_timestamp_ms,
            ids_created: 0,
            rgp,
            gas_price,
            gas_budget,
            sponsor: sponsor.map(|s| s.into()),
            is_native: protocol_config.move_native_context(),
        }
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: TX_CONTEXT_MODULE_NAME.to_owned(),
            name: TX_CONTEXT_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch
    }

    pub fn sender(&self) -> SuiAddress {
        self.sender.into()
    }

    pub fn epoch_timestamp_ms(&self) -> u64 {
        self.epoch_timestamp_ms
    }

    /// Return the transaction digest, to include in new objects
    pub fn digest(&self) -> TransactionDigest {
        TransactionDigest::new(self.digest.clone().try_into().unwrap())
    }

    pub fn sponsor(&self) -> Option<SuiAddress> {
        self.sponsor.map(SuiAddress::from)
    }

    pub fn rgp(&self) -> u64 {
        self.rgp
    }

    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    pub fn gas_budget(&self) -> u64 {
        self.gas_budget
    }

    pub fn ids_created(&self) -> u64 {
        self.ids_created
    }

    /// Derive a globally unique object ID by hashing self.digest | self.ids_created
    pub fn fresh_id(&mut self) -> ObjectID {
        let id = ObjectID::derive_id(self.digest(), self.ids_created);

        self.ids_created += 1;
        id
    }

    pub fn to_bcs_legacy_context(&self) -> Vec<u8> {
        let move_context: MoveLegacyTxContext = if self.is_native {
            let tx_context = &TxContext {
                sender: AccountAddress::ZERO,
                digest: self.digest.clone(),
                epoch: 0,
                epoch_timestamp_ms: 0,
                ids_created: 0,
                rgp: 0,
                gas_price: 0,
                gas_budget: 0,
                sponsor: None,
                is_native: true,
            };
            tx_context.into()
        } else {
            self.into()
        };
        bcs::to_bytes(&move_context).unwrap()
    }

    pub fn to_vec(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    /// Updates state of the context instance. It's intended to use
    /// when mutable context is passed over some boundary via
    /// serialize/deserialize and this is the reason why this method
    /// consumes the other context..
    pub fn update_state(&mut self, other: MoveLegacyTxContext) -> Result<(), ExecutionError> {
        if !self.is_native {
            if self.sender != other.sender
                || self.digest != other.digest
                || other.ids_created < self.ids_created
            {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::InvariantViolation,
                    "Immutable fields for TxContext changed",
                ));
            }
            self.ids_created = other.ids_created;
        }
        Ok(())
    }

    //
    // Move test only API
    //
    pub fn replace(
        &mut self,
        sender: AccountAddress,
        tx_hash: Vec<u8>,
        epoch: u64,
        epoch_timestamp_ms: u64,
        ids_created: u64,
        rgp: u64,
        gas_price: u64,
        gas_budget: u64,
        sponsor: Option<AccountAddress>,
    ) {
        self.sender = sender;
        self.digest = tx_hash;
        self.epoch = epoch;
        self.epoch_timestamp_ms = epoch_timestamp_ms;
        self.ids_created = ids_created;
        self.rgp = rgp;
        self.gas_price = gas_price;
        self.gas_budget = gas_budget;
        self.sponsor = sponsor;
    }
}
