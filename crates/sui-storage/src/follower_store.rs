// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::{
    base_types::AuthorityName,
    batch::TxSequenceNumber,
    error::{SuiError, SuiResult},
};
use typed_store::rocks::DBMap;
use typed_store::traits::DBMapTableUtil;
use typed_store::traits::Map;
use typed_store_macros::DBMapUtils;

use tracing::debug;

/// FollowerStore tracks the next tx sequence numbers that we should expect after the previous
/// batch.
#[derive(DBMapUtils)]
pub struct FollowerStore {
    next_sequence: DBMap<AuthorityName, TxSequenceNumber>,
}

impl FollowerStore {
    pub fn get_next_sequence(&self, name: &AuthorityName) -> SuiResult<Option<TxSequenceNumber>> {
        self.next_sequence.get(name).map_err(SuiError::StorageError)
    }

    pub fn record_next_sequence(&self, name: &AuthorityName, seq: TxSequenceNumber) -> SuiResult {
        debug!(peer = ?name, ?seq, "record_next_sequence");
        self.next_sequence
            .insert(name, &seq)
            .map_err(SuiError::StorageError)
    }
}

#[cfg(test)]
mod test {
    use crate::follower_store::FollowerStore;
    use sui_types::crypto::{get_key_pair, AuthorityKeyPair, KeypairTraits};
    use typed_store::traits::DBMapTableUtil;

    #[test]
    fn test_follower_store() {
        let working_dir = tempfile::tempdir().unwrap();

        let follower_store =
            FollowerStore::open_tables_read_write(working_dir.as_ref().to_path_buf(), None);

        let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
        let val_name = &key_pair.public().into();

        let seq = follower_store
            .get_next_sequence(val_name)
            .expect("read error");
        assert!(seq.is_none());

        follower_store
            .record_next_sequence(val_name, 42)
            .expect("write error");

        let seq = follower_store
            .get_next_sequence(val_name)
            .expect("read error");
        assert_eq!(seq.unwrap(), 42);

        follower_store
            .record_next_sequence(val_name, 43)
            .expect("write error");

        let seq = follower_store
            .get_next_sequence(val_name)
            .expect("read error");
        assert_eq!(seq.unwrap(), 43);
    }
}
