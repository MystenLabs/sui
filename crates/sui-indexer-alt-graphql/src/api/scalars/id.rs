// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::InputValueError;
use async_graphql::Scalar;
use async_graphql::ScalarType;
use async_graphql::Value;
use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use serde::Deserialize;
use serde::Serialize;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::digests::ObjectDigest;
use sui_types::digests::TransactionDigest;

#[derive(Serialize, Deserialize)]
pub(crate) enum Id {
    Address(NativeSuiAddress),
    Checkpoint(u64),
    DynamicFieldByAddress(NativeSuiAddress),
    DynamicFieldByRef(NativeSuiAddress, SequenceNumber, ObjectDigest),
    Epoch(u64),
    MoveObjectByAddress(NativeSuiAddress),
    MoveObjectByRef(NativeSuiAddress, SequenceNumber, ObjectDigest),
    MovePackage(NativeSuiAddress),
    ObjectByAddress(NativeSuiAddress),
    ObjectByRef(NativeSuiAddress, SequenceNumber, ObjectDigest),
    Transaction(TransactionDigest),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Invalid Base64")]
    BadBase64,

    #[error("Invalid BCS: {0}")]
    BadBcs(#[from] bcs::Error),
}

impl Id {
    fn decode(s: &str) -> Result<Self, Error> {
        let bytes = Base64::decode(s).map_err(|_| Error::BadBase64)?;
        Ok(bcs::from_bytes(&bytes)?)
    }

    fn encode(&self) -> String {
        Base64::encode(bcs::to_bytes(self).unwrap_or_default())
    }
}

#[Scalar(name = "ID")]
impl ScalarType for Id {
    fn parse(value: Value) -> async_graphql::InputValueResult<Self> {
        if let Value::String(s) = value {
            Self::decode(&s).map_err(InputValueError::custom)
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn is_valid(value: &Value) -> bool {
        matches!(value, Value::String(_))
    }

    fn to_value(&self) -> Value {
        Value::String(self.encode())
    }
}
