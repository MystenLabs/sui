// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::signature::GenericSignature;

use crate::api::scalars::base64::Base64;

/// A user signature for a transaction.
#[derive(Clone)]
pub(crate) struct UserSignature {
    pub(crate) native: GenericSignature,
}

#[Object]
impl UserSignature {
    /// The signature bytes, Base64-encoded.
    /// For simple signatures: flag || signature || pubkey
    /// For complex signatures: flag || bcs_serialized_struct
    async fn signature_bytes(&self) -> Option<Base64> {
        Some(Base64(self.native.as_ref().to_vec()))
    }
}

// TODO(DVX-786): Support signature scheme details.
impl UserSignature {
    pub(crate) fn from_generic_signature(signature: GenericSignature) -> Self {
        Self { native: signature }
    }
}
