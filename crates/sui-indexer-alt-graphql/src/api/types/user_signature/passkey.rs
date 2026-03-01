// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::passkey_authenticator::PasskeyAuthenticator;

use crate::api::scalars::base64::Base64;

use super::{SignatureScheme, simple_signature_to_scheme};

/// A Passkey signature.
#[derive(Clone)]
pub(crate) struct PasskeySignature {
    native: PasskeyAuthenticator,
}

#[Object]
impl PasskeySignature {
    /// The authenticator data returned by the passkey device.
    async fn authenticator_data(&self) -> Base64 {
        Base64(self.native.authenticator_data().to_vec())
    }

    /// The client data JSON string passed to the authenticator.
    async fn client_data_json(&self) -> &str {
        self.native.client_data_json()
    }

    /// The inner user signature (secp256r1).
    async fn user_signature(&self) -> SignatureScheme {
        simple_signature_to_scheme(&self.native.signature())
    }
}

impl From<&PasskeyAuthenticator> for PasskeySignature {
    fn from(p: &PasskeyAuthenticator) -> Self {
        Self { native: p.clone() }
    }
}
