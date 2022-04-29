// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[path = "generated/sui.validator.rs"]
#[rustfmt::skip]
mod validator;

#[path = "generated/sui.common.rs"]
#[rustfmt::skip]
mod common;

pub use common::BincodeEncodedPayload;
pub use validator::{
    validator_client::ValidatorClient,
    validator_server::{Validator, ValidatorServer},
};

impl BincodeEncodedPayload {
    pub fn deserialize<T: serde::de::DeserializeOwned>(&self) -> Result<T, bincode::Error> {
        bincode::deserialize(self.payload.as_ref())
    }

    pub fn try_from<T: serde::Serialize>(value: &T) -> Result<Self, bincode::Error> {
        let payload = bincode::serialize(value)?.into();
        Ok(Self { payload })
    }
}

impl From<bytes::Bytes> for BincodeEncodedPayload {
    fn from(payload: bytes::Bytes) -> Self {
        Self { payload }
    }
}
