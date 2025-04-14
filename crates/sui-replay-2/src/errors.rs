// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Node;
use std::num::ParseIntError;
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum ReplayError {
    #[error("Generic Error: {:?}", err)]
    GenericError { err: String },

    #[error("BCS Conversion Error: {:?}", err)]
    BCSConversionError { err: String },

    #[error("Failed to parse transaction digest {}: {}", digest, err)]
    FailedToParseDigest { digest: String, err: String },
    #[error("Failed to load transaction {}: {}", digest, err)]
    FailedToLoadTransaction { digest: String, err: String },
    #[error("Transaction {} not found on {:?}", digest, node)]
    TransactionNotFound { digest: String, node: Node },
    #[error("Failed to load transaction effects for {}: {}", digest, err)]
    FailedToLoadTransactionEffects { digest: String, err: String },
    #[error("Transaction effects {} not found on {:?}", digest, node)]
    TransactionEffectsNotFound { digest: String, node: Node },
    #[error("Failed to retrieve change epoch events: {}", err)]
    ChangeEpochEventsFailure { err: String },

    // EpochStore errors
    #[error("Missing {} for epoch {}", data, epoch)]
    MissingDataForEpoch { data: String, epoch: u64 },

    #[error("Missing system package {} at version {}", pkg, epoch)]
    MissingSystemPackage { pkg: String, epoch: u64 },
    #[error("Cannot find epoch for package {} at epoch {}", pkg, epoch)]
    MissingPackageAtEpoch { pkg: String, epoch: u64 },
    #[error("Cannot find package epoch for {}", pkg)]
    MissingPackageEpoch { pkg: String },

    #[error("Failed to get executor: {}", err)]
    ExecutorError { err: String },
    #[error("Failed to get packages for {}: {}", pkg, err)]
    PackagesRetrievalError { pkg: String, err: String },
    #[error("Package not found {}", pkg)]
    PackageNotFound { pkg: String },
    #[error("Cannot load package {}: {}", pkg, err)]
    LoadPackageError { pkg: String, err: String },
    #[error("Failed to load object {}[{:?}]: {}", address, version, err)]
    ObjectLoadError {
        address: String,
        version: Option<u64>,
        err: String,
    },
    #[error("Object not found {}[{:?}]", address, version)]
    ObjectNotFound {
        address: String,
        version: Option<u64>,
    },
    #[error("Object version not found {}[{:?}]", address, version)]
    ObjectVersionNotFound {
        address: String,
        version: Option<u64>,
    },
    #[error("Cannot convert TransactionKind for transaction {}: {:?}", digest, err)]
    TransactionKindError { digest: String, err: String },
    #[error("Cannot find epoch timestamp in transaction {}", digest)]
    NoEpochTimestamp { digest: String },
    #[error(
        "Fail to create object digest {}, for transaction {}: {}",
        object_digest,
        digest,
        err
    )]
    ObjectDigestError {
        digest: String,
        object_digest: String,
        err: String,
    },
    #[error(
        "Fail to create object id {}, for transaction {}: {}",
        object_id,
        digest,
        err
    )]
    ObjectIDError {
        digest: String,
        object_id: String,
        err: String,
    },
    #[error("Failed to create client for host {}: {}", host, err)]
    ClientCreationError { host: String, err: String },
    #[error("Cannot get input objects for transaction {}: {}", digest, err)]
    InputObjectsError { digest: String, err: String },

    #[error("Cannot convert package {}: {}", pkg, err)]
    PackageConversionError { pkg: String, err: String },
    #[error("Cannot convert object {}: {}", id, err)]
    ObjectConversionError { id: String, err: String },
    #[error("Cannot convert transaction {}: {}", id, err)]
    TransactionConversionError { id: String, err: String },
    #[error("Cannot convert transaction effects {}: {}", id, err)]
    TransactionEffectsConversionError { id: String, err: String },

    #[error("Cannot convert DateTime millis (i64) to u64")]
    DateTimeConversionError,
    #[error("Failed to parse integer: {0}")]
    ParseIntConversionError(#[from] ParseIntError),

    #[error("Tracing error: {}", err)]
    TracingError { err: String },

    #[error("{}", err)]
    DynamicFieldError { err: String },
}
