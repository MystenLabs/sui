// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, TransactionDigest},
    batch::TxSequenceNumber,
    error::SuiResult,
    messages::{CertifiedTransaction, SignedTransactionEffects},
};

use typed_store::rocks::DBMap;
use typed_store::traits::DBMapTableUtil;
use typed_store::traits::Map;
use typed_store_macros::DBMapUtils;

use tracing::trace;

/// NodeSyncStore store is used by nodes to store downloaded objects (certs, etc) that have
/// not yet been applied to the node's SuiDataStore.
#[derive(DBMapUtils)]
pub struct NodeSyncStore {
    /// Certificates/Effects that have been fetched from remote validators, but not sequenced.
    certs_and_fx: DBMap<TransactionDigest, (CertifiedTransaction, SignedTransactionEffects)>,

    /// The persisted batch streams (minus the signed batches) from each authority.
    batch_streams: DBMap<(AuthorityName, TxSequenceNumber), ExecutionDigests>,

    /// The latest received sequence from each authority.
    latest_seq: DBMap<AuthorityName, TxSequenceNumber>,
}

impl NodeSyncStore {
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

    pub fn enqueue_execution_digests(
        &self,
        peer: AuthorityName,
        seq: TxSequenceNumber,
        digests: &ExecutionDigests,
    ) -> SuiResult {
        let mut write_batch = self.batch_streams.batch();
        trace!(?peer, ?seq, ?digests, "persisting digests to db");
        write_batch = write_batch
            .insert_batch(&self.batch_streams, std::iter::once(((peer, seq), digests)))?;

        match self.latest_seq.get(&peer)? {
            // Note: this can actually happen, because when you request a starting sequence
            // from the validator, it sends you any preceding txes that were in the same
            // batch.
            Some(prev_latest) if prev_latest > seq => (),

            _ => {
                trace!(?peer, ?seq, "recording latest sequence to db");
                write_batch =
                    write_batch.insert_batch(&self.latest_seq, std::iter::once((peer, seq)))?;
            }
        }

        write_batch.write()?;
        Ok(())
    }

    pub fn batch_stream_iter<'a>(
        &'a self,
        peer: &'a AuthorityName,
    ) -> SuiResult<impl Iterator<Item = (TxSequenceNumber, ExecutionDigests)> + 'a> {
        Ok(self
            .batch_streams
            .iter()
            .skip_to(&(*peer, 0))?
            .take_while(|((name, _), _)| *name == *peer)
            .map(|((_, seq), digests)| (seq, digests)))
    }

    pub fn latest_seq_for_peer(&self, peer: &AuthorityName) -> SuiResult<Option<TxSequenceNumber>> {
        Ok(self.latest_seq.get(peer)?)
    }

    pub fn remove_batch_stream_item(
        &self,
        peer: AuthorityName,
        seq: TxSequenceNumber,
    ) -> SuiResult {
        Ok(self.batch_streams.remove(&(peer, seq))?)
    }
}
