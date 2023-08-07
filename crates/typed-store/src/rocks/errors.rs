// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bincode::ErrorKind as BincodeErrorKind;

use rocksdb::Error as RocksError;
use serde::{Deserialize, Serialize};
use std::{fmt, fmt::Display};
use thiserror::Error;

#[non_exhaustive]
#[derive(Error, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Ord, PartialOrd)]
pub enum TypedStoreError {
    #[error("rocksdb error: {0}")]
    RocksDBError(String),
    #[error("(de)serialization error: {0}")]
    SerializationError(String),
    #[error("the column family {0} was not registered with the database")]
    UnregisteredColumn(String),
    #[error("a batch operation can't operate across databases")]
    CrossDBBatch,
    #[error("Metric reporting thread failed with error")]
    MetricsReporting,
    #[error("Transaction should be retried")]
    RetryableTransactionError,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Hash, Debug, Error)]
pub(crate) struct RocksErrorDef {
    message: String,
}

impl From<RocksError> for RocksErrorDef {
    fn from(err: RocksError) -> Self {
        RocksErrorDef {
            message: err.as_ref().to_string(),
        }
    }
}

impl From<RocksError> for TypedStoreError {
    fn from(err: RocksError) -> Self {
        TypedStoreError::RocksDBError(format!("{err}"))
    }
}

impl Display for RocksErrorDef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        self.message.fmt(formatter)
    }
}

#[derive(Serialize, Deserialize, Clone, Hash, Eq, PartialEq, Debug, Error)]
pub(crate) enum BincodeErrorDef {
    Io(String),
    InvalidUtf8Encoding(String),
    InvalidBoolEncoding(u8),
    InvalidCharEncoding,
    InvalidTagEncoding(usize),
    DeserializeAnyNotSupported,
    SizeLimit,
    SequenceMustHaveLength,
    Custom(String),
}

impl fmt::Display for BincodeErrorDef {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            BincodeErrorDef::Io(ref ioerr) => write!(fmt, "io error: {ioerr}"),
            BincodeErrorDef::InvalidUtf8Encoding(ref e) => {
                write!(fmt, "{e}")
            }
            BincodeErrorDef::InvalidBoolEncoding(b) => {
                write!(fmt, "expected 0 or 1, found {b}")
            }
            BincodeErrorDef::InvalidCharEncoding => write!(fmt, "{self:?}"),
            BincodeErrorDef::InvalidTagEncoding(tag) => {
                write!(fmt, "found {tag}")
            }
            BincodeErrorDef::SequenceMustHaveLength => write!(fmt, "{self:?}"),
            BincodeErrorDef::SizeLimit => write!(fmt, "{self:?}"),
            BincodeErrorDef::DeserializeAnyNotSupported => write!(
                fmt,
                "Bincode does not support the serde::Deserializer::deserialize_any method"
            ),
            BincodeErrorDef::Custom(ref s) => s.fmt(fmt),
        }
    }
}

impl From<bincode::Error> for BincodeErrorDef {
    fn from(err: bincode::Error) -> Self {
        match err.as_ref() {
            BincodeErrorKind::Io(ioerr) => BincodeErrorDef::Io(ioerr.to_string()),
            BincodeErrorKind::InvalidUtf8Encoding(utf8err) => {
                BincodeErrorDef::InvalidUtf8Encoding(utf8err.to_string())
            }
            BincodeErrorKind::InvalidBoolEncoding(byte) => {
                BincodeErrorDef::InvalidBoolEncoding(*byte)
            }
            BincodeErrorKind::InvalidCharEncoding => BincodeErrorDef::InvalidCharEncoding,
            BincodeErrorKind::InvalidTagEncoding(tag) => BincodeErrorDef::InvalidTagEncoding(*tag),
            BincodeErrorKind::DeserializeAnyNotSupported => {
                BincodeErrorDef::DeserializeAnyNotSupported
            }
            BincodeErrorKind::SizeLimit => BincodeErrorDef::SizeLimit,
            BincodeErrorKind::SequenceMustHaveLength => BincodeErrorDef::SequenceMustHaveLength,
            BincodeErrorKind::Custom(str) => BincodeErrorDef::Custom(str.to_owned()),
        }
    }
}

impl From<bcs::Error> for TypedStoreError {
    fn from(err: bcs::Error) -> Self {
        TypedStoreError::SerializationError(format!("{err}"))
    }
}

impl From<bincode::Error> for TypedStoreError {
    fn from(err: bincode::Error) -> Self {
        TypedStoreError::SerializationError(format!("{err}"))
    }
}
