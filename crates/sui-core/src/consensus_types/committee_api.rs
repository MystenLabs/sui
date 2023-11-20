// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::consensus_types::AuthorityIndex;
use narwhal_config::committee::AuthorityIdentifier;
use sui_types::base_types::AuthorityName;
use sui_types::committee::StakeUnit;

pub(crate) trait CommitteeAPI {
    fn total_stake(&self) -> StakeUnit;
    fn authority_pubkey_by_index(&self, index: AuthorityIndex) -> Option<AuthorityName>;
    fn authority_hostname_by_index(&self, index: AuthorityIndex) -> Option<&str>;
    fn authority_stake_by_index(&self, index: AuthorityIndex) -> StakeUnit;
}

impl CommitteeAPI for narwhal_config::Committee {
    fn total_stake(&self) -> StakeUnit {
        narwhal_config::Committee::total_stake(self)
    }

    fn authority_pubkey_by_index(&self, index: AuthorityIndex) -> Option<AuthorityName> {
        let id = AuthorityIdentifier(index);
        self.authority(&id).map(|authority| {
            let name: AuthorityName = authority.protocol_key().into();
            name
        })
    }

    fn authority_hostname_by_index(&self, index: AuthorityIndex) -> Option<&str> {
        let id = AuthorityIdentifier(index);
        self.authority(&id).map(|authority| authority.hostname())
    }

    fn authority_stake_by_index(&self, index: AuthorityIndex) -> StakeUnit {
        let id = AuthorityIdentifier(index);
        self.authority(&id)
            .map(|authority| authority.stake())
            .unwrap_or(0)
    }
}
