// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use async_graphql::Object;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;

use crate::api::{scalars::base64::Base64, types::epoch::Epoch};
use crate::error::RpcError;
use crate::scope::Scope;

/// Represents an aggregated signature from multiple validators.
#[derive(Clone)]
pub(crate) struct ValidatorAggregatedSignature {
    authority: AuthorityStrongQuorumSignInfo,
    scope: Scope,
}

#[Object]
impl ValidatorAggregatedSignature {
    /// The epoch when this aggregate signature was produced.
    async fn epoch(&self) -> Option<Epoch> {
        Some(Epoch::with_id(self.scope.clone(), self.authority.epoch))
    }

    /// The 48-byte BLS12381 aggregated signature, encoded in Base64.
    async fn signature(&self) -> Result<Option<Base64>, RpcError> {
        let signature_bytes = bcs::to_bytes(&self.authority.signature)
            .context("Failed to serialize aggregated signature")?;
        Ok(Some(Base64::from(signature_bytes)))
    }

    /// The indexes of validators that contributed to this signature.
    async fn signers_map(&self) -> Vec<u32> {
        self.authority.signers_map.iter().collect()
    }
}

impl From<(AuthorityStrongQuorumSignInfo, Scope)> for ValidatorAggregatedSignature {
    fn from((authority, scope): (AuthorityStrongQuorumSignInfo, Scope)) -> Self {
        Self { authority, scope }
    }
}
