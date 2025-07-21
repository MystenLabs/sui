// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::scalars::base64::Base64;
use crate::api::types::epoch::Epoch;
use crate::scope::Scope;
use async_graphql::Object;

use sui_types::crypto::AuthorityStrongQuorumSignInfo;

/// Represents an aggregated signature from multiple validators.
#[derive(Clone)]
pub(crate) struct ValidatorAggregatedSignature {
    authority_info: AuthorityStrongQuorumSignInfo,
    scope: Scope,
}

#[Object]
impl ValidatorAggregatedSignature {
    /// The epoch when this aggregate signature was produced.
    async fn epoch(&self) -> Option<Epoch> {
        Some(Epoch::with_id(
            self.scope.clone(),
            self.authority_info.epoch,
        ))
    }

    /// The Base64 encoded BLS12381 aggregated signature.
    async fn signature(&self) -> Option<Base64> {
        Some(Base64::from(self.authority_info.signature.as_ref()))
    }

    /// The indexes of validators that contributed to this signature.
    async fn signers_map(&self) -> Vec<u32> {
        self.authority_info.signers_map.iter().collect()
    }
}

impl ValidatorAggregatedSignature {
    pub(crate) fn with_authority_info(
        scope: Scope,
        authority_info: AuthorityStrongQuorumSignInfo,
    ) -> Self {
        Self {
            authority_info,
            scope,
        }
    }
}
