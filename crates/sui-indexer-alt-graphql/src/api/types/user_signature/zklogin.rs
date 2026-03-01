// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;

use crate::api::scalars::uint53::UInt53;

use super::{SignatureScheme, simple_signature_to_scheme};

/// A zkLogin signature.
#[derive(Clone)]
pub(crate) struct ZkLoginSignature {
    native: ZkLoginAuthenticator,
}

#[Object]
impl ZkLoginSignature {
    /// The maximum epoch for which this signature is valid.
    async fn max_epoch(&self) -> UInt53 {
        self.native.get_max_epoch().into()
    }

    /// The inner user signature (ed25519/secp256k1/secp256r1).
    async fn user_signature(&self) -> SignatureScheme {
        simple_signature_to_scheme(&self.native.user_signature)
    }
}

impl From<&ZkLoginAuthenticator> for ZkLoginSignature {
    fn from(z: &ZkLoginAuthenticator) -> Self {
        Self { native: z.clone() }
    }
}
