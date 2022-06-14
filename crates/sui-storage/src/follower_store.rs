// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use sui_types::{
    base_types::AuthorityName,
    batch::TxSequenceNumber,
    error::{SuiError, SuiResult},
};
use typed_store::rocks::DBMap;
use typed_store::{reopen, traits::Map};

use crate::default_db_options;

use tracing::debug;

/// FollowerStore tracks the next tx sequence numbers that we should expect after the previous
/// batch.
pub struct FollowerStore {
    next_sequence: DBMap<AuthorityName, TxSequenceNumber>,
}

impl FollowerStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, SuiError> {
        let (options, _) = default_db_options(None, None);

        let db = {
            let path = &path;
            let db_options = Some(options.clone());
            let opt_cfs: &[(&str, &rocksdb::Options)] = &[("next_sequence", &options)];
            typed_store::rocks::open_cf_opts(path, db_options, opt_cfs)
        }
        .map_err(SuiError::StorageError)?;

        let next_sequence = reopen!(&db, "next_sequence";<AuthorityName, TxSequenceNumber>);

        Ok(Self { next_sequence })
    }

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
    use sui_types::crypto::get_key_pair;

    #[test]
    fn test_follower_store() {
        let working_dir = tempfile::tempdir().unwrap();

        let follower_store = FollowerStore::open(&working_dir).expect("cannot open db");

        let (_, key_pair) = get_key_pair();
        let val_name = key_pair.public_key_bytes();

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
