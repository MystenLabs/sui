// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::default_db_options;
use std::path::Path;

use sui_types::{
    base_types::TransactionDigest,
    error::{SuiError, SuiResult},
    messages::{CertifiedTransaction, SignedTransactionEffects},
};

use typed_store::rocks::DBMap;
use typed_store::{reopen, traits::Map};

/// NodeSyncStore store is used by nodes to store downloaded objects (certs, etc) that have
/// not yet been applied to the node's SuiDataStore.
pub struct NodeSyncStore {
    /// Certificates/Effects that have been fetched from remote validators, but not sequenced.
    certs_and_fx: DBMap<TransactionDigest, (CertifiedTransaction, SignedTransactionEffects)>,
}

impl NodeSyncStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, SuiError> {
        let (options, _) = default_db_options(None, None);

        let db = {
            let path = &path;
            let db_options = Some(options.clone());
            let opt_cfs: &[(&str, &rocksdb::Options)] = &[("certs_and_fx", &options)];
            typed_store::rocks::open_cf_opts(path, db_options, opt_cfs)
        }
        .map_err(SuiError::StorageError)?;

        let certs_and_fx = reopen!(&db, "certs_and_fx";<TransactionDigest, (CertifiedTransaction, SignedTransactionEffects)>);

        Ok(Self { certs_and_fx })
    }

    pub fn has_cert_and_effects(&self, tx: &TransactionDigest) -> SuiResult<bool> {
        Ok(self.certs_and_fx.contains_key(tx)?)
    }

    pub fn store_cert_and_effects(
        &self,
        tx: &TransactionDigest,
        val: &(CertifiedTransaction, SignedTransactionEffects),
    ) -> SuiResult {
        Ok(self.certs_and_fx.insert(tx, val)?)
    }

    pub fn get_cert_and_effects(
        &self,
        tx: &TransactionDigest,
    ) -> SuiResult<Option<(CertifiedTransaction, SignedTransactionEffects)>> {
        Ok(self.certs_and_fx.get(tx)?)
    }

    pub fn delete_cert_and_effects(&self, tx: &TransactionDigest) -> SuiResult {
        Ok(self.certs_and_fx.remove(tx)?)
    }
}
