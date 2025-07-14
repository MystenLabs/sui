// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bincode::ErrorKind as BincodeErrorKind;

use rocksdb::Error as RocksError;
use serde::{Deserialize, Serialize};
use std::{fmt, fmt::Display};
use thiserror::Error;
use typed_store_error::TypedStoreError;

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Hash, Debug, Error)]
pub(crate) struct RocksErrorDef {
    message: String,
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

pub fn typed_store_err_from_bincode_err(err: bincode::Error) -> TypedStoreError {
    TypedStoreError::SerializationError(format!("{err}"))
}

pub fn typed_store_err_from_bcs_err(err: bcs::Error) -> TypedStoreError {
    TypedStoreError::SerializationError(format!("{err}"))
}

pub fn typed_store_err_from_rocks_err(err: RocksError) -> TypedStoreError {
    TypedStoreError::RocksDBError(format!("{err}"))
}
