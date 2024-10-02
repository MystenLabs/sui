// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    effects::{SignedTransactionEffects, TransactionEvents},
    transaction::{CertifiedTransaction, SignedTransaction, Transaction},
};

/// This enum represents all possible states of a response returned from
/// the safe client. Note that [struct SignedTransaction] and
/// [struct SignedTransactionEffects] are represented as an Envelope
/// instead of an VerifiedEnvelope. This is because the verification is
/// now performed by the authority aggregator as an aggregated signature,
/// instead of in SafeClient.
#[derive(Clone, Debug)]
pub enum PlainTransactionInfoResponse {
    Signed(SignedTransaction),
    ExecutedWithCert(
        CertifiedTransaction,
        SignedTransactionEffects,
        TransactionEvents,
    ),
    ExecutedWithoutCert(Transaction, SignedTransactionEffects, TransactionEvents),
}

impl PlainTransactionInfoResponse {
    pub fn is_executed(&self) -> bool {
        match self {
            PlainTransactionInfoResponse::Signed(_) => false,
            PlainTransactionInfoResponse::ExecutedWithCert(_, _, _)
            | PlainTransactionInfoResponse::ExecutedWithoutCert(_, _, _) => true,
        }
    }
}
