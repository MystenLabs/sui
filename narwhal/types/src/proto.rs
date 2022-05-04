// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[path = "generated/narwhal.rs"]
#[rustfmt::skip]
mod narwhal;

use std::{array::TryFromSliceError, ops::Deref};

use crate::{Batch, BatchMessage, BlockError, BlockErrorType, CertificateDigest};
use bytes::Bytes;

pub use narwhal::{
    collection_retrieval_result::RetrievalResult,
    primary_to_primary_client::PrimaryToPrimaryClient,
    primary_to_primary_server::{PrimaryToPrimary, PrimaryToPrimaryServer},
    primary_to_worker_client::PrimaryToWorkerClient,
    primary_to_worker_server::{PrimaryToWorker, PrimaryToWorkerServer},
    transactions_client::TransactionsClient,
    transactions_server::{Transactions, TransactionsServer},
    validator_client::ValidatorClient,
    validator_server::{Validator, ValidatorServer},
    worker_to_primary_client::WorkerToPrimaryClient,
    worker_to_primary_server::{WorkerToPrimary, WorkerToPrimaryServer},
    worker_to_worker_client::WorkerToWorkerClient,
    worker_to_worker_server::{WorkerToWorker, WorkerToWorkerServer},
    Batch as BatchProto, BatchDigest as BatchDigestProto, BatchMessage as BatchMessageProto,
    BincodeEncodedPayload, CertificateDigest as CertificateDigestProto, CollectionError,
    CollectionErrorType, CollectionRetrievalResult, Empty, GetCollectionsRequest,
    GetCollectionsResponse, Transaction as TransactionProto,
};

impl From<BatchMessage> for BatchMessageProto {
    fn from(message: BatchMessage) -> Self {
        BatchMessageProto {
            id: Some(message.id.into()),
            transactions: Some(message.transactions.into()),
        }
    }
}

impl From<Batch> for BatchProto {
    fn from(batch: Batch) -> Self {
        BatchProto {
            transaction: batch
                .0
                .into_iter()
                .map(|transaction| TransactionProto {
                    transaction: Bytes::from(transaction),
                })
                .collect::<Vec<TransactionProto>>(),
        }
    }
}

impl From<BlockError> for CollectionError {
    fn from(error: BlockError) -> Self {
        CollectionError {
            id: Some(error.id.into()),
            error: CollectionErrorType::from(error.error).into(),
        }
    }
}

impl From<BlockErrorType> for CollectionErrorType {
    fn from(error_type: BlockErrorType) -> Self {
        match error_type {
            BlockErrorType::BlockNotFound => CollectionErrorType::CollectionNotFound,
            BlockErrorType::BatchTimeout => CollectionErrorType::CollectionTimeout,
            BlockErrorType::BatchError => CollectionErrorType::CollectionError,
        }
    }
}

impl TryFrom<CertificateDigestProto> for CertificateDigest {
    type Error = TryFromSliceError;

    fn try_from(digest: CertificateDigestProto) -> Result<Self, Self::Error> {
        Ok(CertificateDigest::new(digest.digest.deref().try_into()?))
    }
}

impl BincodeEncodedPayload {
    pub fn deserialize<T: serde::de::DeserializeOwned>(&self) -> Result<T, bincode::Error> {
        bincode::deserialize(self.payload.as_ref())
    }

    pub fn try_from<T: serde::Serialize>(value: &T) -> Result<Self, bincode::Error> {
        let payload = bincode::serialize(value)?.into();
        Ok(Self { payload })
    }
}

impl From<Bytes> for BincodeEncodedPayload {
    fn from(payload: Bytes) -> Self {
        Self { payload }
    }
}
