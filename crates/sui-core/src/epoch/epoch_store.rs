// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_types::committee::{Committee, EpochId};
use sui_types::error::SuiResult;
use sui_types::messages::{AuthenticatedEpoch, GenesisEpoch};
use typed_store::rocks::DBMap;
use typed_store::traits::DBMapTableUtil;
use typed_store::Map;
use typed_store_macros::DBMapUtils;

#[derive(DBMapUtils)]
pub struct EpochStore {
    /// Map from each epoch ID to the epoch information. The epoch is either signed by this node,
    /// or is certified (signed by a quorum).
    #[options(optimization = "point_lookup")]
    pub(crate) epochs: DBMap<EpochId, AuthenticatedEpoch>,
}

impl EpochStore {
    pub fn new(path: PathBuf) -> Self {
        Self::open_tables_read_write(path, None)
    }

    pub fn database_is_empty(&self) -> bool {
        self.epochs.iter().next().is_none()
    }

    pub fn init_genesis_epoch(&self, genesis_committee: Committee) -> SuiResult {
        assert_eq!(genesis_committee.epoch, 0);
        let epoch_data = AuthenticatedEpoch::Genesis(GenesisEpoch::new(genesis_committee));
        self.epochs.insert(&0, &epoch_data)?;
        Ok(())
    }

    pub fn get_authenticated_epoch(
        &self,
        epoch_id: &EpochId,
    ) -> SuiResult<Option<AuthenticatedEpoch>> {
        Ok(self.epochs.get(epoch_id)?)
    }

    pub fn get_latest_authenticated_epoch(&self) -> AuthenticatedEpoch {
        self.epochs
            .iter()
            .skip_to_last()
            .next()
            // unwrap safe because we guarantee there is at least a genesis epoch
            // when initializing the store.
            .unwrap()
            .1
    }
}
