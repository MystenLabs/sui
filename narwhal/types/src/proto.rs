// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[path = "generated/narwhal.rs"]
#[rustfmt::skip]
mod narwhal;
use crypto::{ed25519::Ed25519PublicKey, traits::ToFromBytes};

use std::{array::TryFromSliceError, ops::Deref};

use crate::{Batch, BatchMessage, BlockError, BlockErrorKind, CertificateDigest};
use bytes::Bytes;

pub use narwhal::{
    collection_retrieval_result::RetrievalResult,
    configuration_client::ConfigurationClient,
    configuration_server::{Configuration, ConfigurationServer},
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
    GetCollectionsResponse, MultiAddr as MultiAddrProto, NewNetworkInfoRequest,
    PublicKey as PublicKeyProto, RemoveCollectionsRequest, Transaction as TransactionProto,
    ValidatorData,
};

impl From<Ed25519PublicKey> for PublicKeyProto {
    fn from(pub_key: Ed25519PublicKey) -> Self {
        PublicKeyProto {
            bytes: Bytes::from(pub_key.as_ref().to_vec()),
        }
    }
}

impl TryFrom<&PublicKeyProto> for Ed25519PublicKey {
    type Error = crypto::traits::Error;

    fn try_from(pub_key: &PublicKeyProto) -> Result<Self, Self::Error> {
        Ed25519PublicKey::from_bytes(pub_key.bytes.as_ref())
    }
}

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

impl From<BlockErrorKind> for CollectionErrorType {
    fn from(error_type: BlockErrorKind) -> Self {
        match error_type {
            BlockErrorKind::BlockNotFound => CollectionErrorType::CollectionNotFound,
            BlockErrorKind::BatchTimeout => CollectionErrorType::CollectionTimeout,
            BlockErrorKind::BatchError => CollectionErrorType::CollectionError,
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
