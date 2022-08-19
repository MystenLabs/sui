// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, TransactionDigest, TransactionEffectsDigest},
    batch::TxSequenceNumber,
    committee::StakeUnit,
    error::SuiResult,
    messages::{CertifiedTransaction, SignedTransactionEffects},
};

use typed_store::rocks::DBMap;
use typed_store::traits::DBMapTableUtil;
use typed_store::traits::Map;
use typed_store_macros::DBMapUtils;

use tracing::trace;

#[cfg(test)]
use std::sync::Arc;

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

    /// Which peers have claimed to have executed which effects?
    effects_votes: DBMap<(TransactionEffectsDigest, AuthorityName), StakeUnit>,
}

impl NodeSyncStore {
    #[cfg(test)]
    pub fn new_for_test() -> Arc<Self> {
        let working_dir = tempfile::tempdir().unwrap();
        let db_path = working_dir.path().join("sync_store");
        Arc::new(NodeSyncStore::open_tables_read_write(db_path, None, None))
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

    pub fn cleanup_cert(
        &self,
        digest: &TransactionDigest,
        effects: Option<&TransactionEffectsDigest>,
    ) -> SuiResult {
        self.certs_and_fx.remove(digest)?;
        if let Some(effects) = effects {
            self.clear_effects_votes(*effects)?;
        }

        Ok(())
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

    pub fn record_effects_vote(
        &self,
        peer: AuthorityName,
        effects_digest: TransactionEffectsDigest,
        stake: StakeUnit,
    ) -> SuiResult {
        Ok(self.effects_votes.insert(&(effects_digest, peer), &stake)?)
    }

    fn iter_fx_digest(
        &self,
        digest: TransactionEffectsDigest,
    ) -> SuiResult<impl Iterator<Item = ((TransactionEffectsDigest, AuthorityName), StakeUnit)> + '_>
    {
        Ok(self
            .effects_votes
            .iter()
            .skip_to(&(digest, AuthorityName::ZERO))?
            .take_while(move |((d, _), _)| *d == digest))
    }

    pub fn count_effects_votes(&self, digest: TransactionEffectsDigest) -> SuiResult<StakeUnit> {
        Ok(self
            .iter_fx_digest(digest)?
            .map(|((_, _), stake)| stake)
            .sum())
    }

    pub fn get_voters(
        &self,
        digest: TransactionEffectsDigest,
    ) -> SuiResult<BTreeSet<AuthorityName>> {
        Ok(self
            .iter_fx_digest(digest)?
            .map(|((_, peer), _)| peer)
            .collect())
    }

    pub fn clear_effects_votes(&self, digest: TransactionEffectsDigest) -> SuiResult {
        Ok(self
            .effects_votes
            .multi_remove(self.iter_fx_digest(digest)?.map(|(k, _)| k))?)
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use sui_types::crypto::{get_authority_key_pair, KeypairTraits};

    #[test]
    fn test_stake_votes() {
        let db = NodeSyncStore::new_for_test();

        let (_, kp1) = get_authority_key_pair();
        let (_, kp2) = get_authority_key_pair();
        let peer1: AuthorityName = kp1.public().into();
        let peer2: AuthorityName = kp2.public().into();

        let digest1 = TransactionEffectsDigest::random();
        let digest2 = TransactionEffectsDigest::random();

        db.record_effects_vote(peer1, digest1, 1).unwrap();
        assert_eq!(db.count_effects_votes(digest1).unwrap(), 1);

        db.record_effects_vote(peer2, digest1, 2).unwrap();
        assert_eq!(db.count_effects_votes(digest1).unwrap(), 3);

        assert_eq!(
            db.get_voters(digest1).unwrap(),
            [peer1, peer2].iter().cloned().collect()
        );

        // redundant votes do not increase total
        db.record_effects_vote(peer2, digest1, 2).unwrap();
        assert_eq!(db.count_effects_votes(digest1).unwrap(), 3);

        db.record_effects_vote(peer1, digest2, 1).unwrap();
        db.record_effects_vote(peer2, digest2, 2).unwrap();

        db.clear_effects_votes(digest1).unwrap();
        // digest1 is cleared
        assert_eq!(db.count_effects_votes(digest1).unwrap(), 0);
        // digest2 is not
        assert_eq!(db.count_effects_votes(digest2).unwrap(), 3);
    }
}
