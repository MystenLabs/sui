// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

use crate::{base_types::*, messages::*};
use serde::{Deserialize, Serialize};

#[macro_export]
macro_rules! fp_bail {
    ($e:expr) => {
        return Err($e)
    };
}

#[macro_export(local_inner_macros)]
macro_rules! fp_ensure {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            fp_bail!($e);
        }
    };
}
pub(crate) use fp_ensure;

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash)]
/// Custom error type for FastPay.

#[allow(clippy::large_enum_variant)]
pub enum FastPayError {
    // Signature verification
    #[error("Signature is not valid: {}", error)]
    InvalidSignature { error: String },
    #[error("Value was not signed by the correct sender")]
    IncorrectSigner,
    #[error("Value was not signed by a known authority")]
    UnknownSigner,
    // Certificate verification
    #[error("Signatures in a certificate must form a quorum")]
    CertificateRequiresQuorum,
    // Transfer processing
    #[error("Transfers must have positive amount")]
    IncorrectTransferAmount,
    #[error(
        "The given sequence number must match the next expected sequence number of the account"
    )]
    UnexpectedSequenceNumber,
    #[error(
         "The transferred amount must be not exceed the current account balance: {current_balance:?}"
    )]
    InsufficientFunding { current_balance: Balance },
    #[error(
          "Cannot initiate transfer while a transfer order is still pending confirmation: {pending_confirmation:?}"
    )]
    PreviousTransferMustBeConfirmedFirst { pending_confirmation: Order },
    #[error("Transfer order was processed but no signature was produced by authority")]
    ErrorWhileProcessingTransferOrder,
    #[error("An invalid answer was returned by the authority while requesting a certificate")]
    ErrorWhileRequestingCertificate,
    #[error(
         "Cannot confirm a transfer while previous transfer orders are still pending confirmation: {current_sequence_number:?}"
    )]
    MissingEalierConfirmations {
        current_sequence_number: VersionNumber,
    },
    // Synchronization validation
    #[error("Transaction index must increase by one")]
    UnexpectedTransactionIndex,
    // Account access
    #[error("No certificate for this account and sequence number")]
    CertificateNotfound,
    #[error("Unknown sender's account")]
    UnknownSenderAccount,
    #[error("Signatures in a certificate must be from different authorities.")]
    CertificateAuthorityReuse,
    #[error("Sequence numbers above the maximal value are not usable for transfers.")]
    InvalidSequenceNumber,
    #[error("Sequence number overflow.")]
    SequenceOverflow,
    #[error("Sequence number underflow.")]
    SequenceUnderflow,
    #[error("Amount overflow.")]
    AmountOverflow,
    #[error("Amount underflow.")]
    AmountUnderflow,
    #[error("Account balance overflow.")]
    BalanceOverflow,
    #[error("Account balance underflow.")]
    BalanceUnderflow,
    #[error("Wrong shard used.")]
    WrongShard,
    #[error("Invalid cross shard update.")]
    InvalidCrossShardUpdate,
    #[error("Cannot deserialize.")]
    InvalidDecoding,
    #[error("Unexpected message.")]
    UnexpectedMessage,
    #[error("Network error while querying service: {:?}.", error)]
    ClientIoError { error: String },

    // Move related errors
    #[error("Failed to load the Move module, reason: {error:?}.")]
    ModuleLoadFailure { error: String },
    #[error("Failed to verify the Move module, reason: {error:?}.")]
    ModuleVerificationFailure { error: String },

    // Internal state errors
    #[error("Attempt to re-initialize an order lock.")]
    OrderLockExists,
    #[error("Attempt to set an non-existing order lock.")]
    OrderLockDoesNotExist,
    #[error("Attempt to reset a set order lock to a different value.")]
    OrderLockReset,
    #[error("Could not find the referenced object.")]
    ObjectNotFound,
}

pub type FastPayResult<T = ()> = Result<T, FastPayError>;
