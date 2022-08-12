// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rocksdb::Options;
use std::path::PathBuf;
use sui_types::base_types::ObjectID;
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
    pub fn new(path: PathBuf, genesis_committee: &Committee, db_options: Option<Options>) -> Self {
        let epoch_store = Self::open_tables_read_write(path, db_options);
        if epoch_store.database_is_empty() {
            epoch_store
                .init_genesis_epoch(genesis_committee.clone())
                .expect("Init genesis epoch data must not fail");
        }
        epoch_store
    }

    pub fn new_for_testing(genesis_committee: &Committee) -> Self {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        std::fs::create_dir(&path).unwrap();
        Self::new(path, genesis_committee, None)
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

    fn database_is_empty(&self) -> bool {
        self.epochs.iter().next().is_none()
    }
}
