// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::passkey_authenticator::PasskeyAuthenticator;

use crate::api::scalars::base64::Base64;

use super::{SignatureScheme, simple_signature_to_scheme};

/// A Passkey signature.
#[derive(Clone)]
pub(crate) struct PasskeySignature {
    pub(super) native: PasskeyAuthenticator,
}

#[Object]
impl PasskeySignature {
    /// The authenticator data returned by the passkey device.
    async fn authenticator_data(&self) -> Option<Base64> {
        Some(Base64(self.native.authenticator_data().to_vec()))
    }

    /// The client data JSON string passed to the authenticator.
    async fn client_data_json(&self) -> Option<String> {
        Some(self.native.client_data_json().to_owned())
    }

    /// The inner user signature (secp256r1).
    async fn user_signature(&self) -> Option<SignatureScheme> {
        Some(simple_signature_to_scheme(&self.native.signature()))
    }
}
