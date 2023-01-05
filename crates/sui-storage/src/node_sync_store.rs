// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use sui_types::{
    base_types::{
        AuthorityName, EpochId, ExecutionDigests, TransactionDigest, TransactionEffectsDigest,
    },
    batch::TxSequenceNumber,
    committee::StakeUnit,
    error::SuiResult,
    messages::{SignedTransactionEffects, TrustedCertificate, VerifiedCertificate},
};

use typed_store::rocks::DBMap;

use typed_store::traits::Map;
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store_derive::DBMapUtils;

use tracing::trace;

#[cfg(test)]
use std::sync::Arc;

/// NodeSyncStore store is used by nodes to store downloaded objects (pending_certs, etc) that have
/// not yet been applied to the node's SuiDataStore.
#[derive(DBMapUtils)]
pub struct NodeSyncStore {
    /// Certificates that have been fetched from remote validators, but not sequenced.
    /// Entries are cleared after execution.
    pending_certs: DBMap<(EpochId, TransactionDigest), TrustedCertificate>,

    /// Verified true effects.
    /// Entries are cleared after execution.
    pending_effects: DBMap<(EpochId, TransactionDigest), SignedTransactionEffects>,

    /// The persisted batch streams (minus the signed batches) from each authority.
    batch_streams: DBMap<(EpochId, AuthorityName, TxSequenceNumber), ExecutionDigests>,

    /// The latest received sequence from each authority.
    latest_seq: DBMap<(EpochId, AuthorityName), TxSequenceNumber>,

    /// Which peers have claimed to have executed which effects?
    effects_votes: DBMap<
        (
            EpochId,
            TransactionDigest,
            TransactionEffectsDigest,
            AuthorityName,
        ),
        StakeUnit,
    >,
}

impl NodeSyncStore {
    #[cfg(test)]
    pub fn new_for_test() -> Arc<Self> {
        let working_dir = tempfile::tempdir().unwrap();
        let db_path = working_dir.path().join("sync_store");
        Arc::new(NodeSyncStore::open_tables_read_write(db_path, None, None))
    }

    pub fn store_cert(&self, epoch_id: EpochId, cert: &VerifiedCertificate) -> SuiResult {
        Ok(self
            .pending_certs
            .insert(&(epoch_id, *cert.digest()), cert.serializable_ref())?)
    }

    pub fn batch_store_certs(&self, certs: impl Iterator<Item = VerifiedCertificate>) -> SuiResult {
        let batch = self.pending_certs.batch().insert_batch(
            &self.pending_certs,
            certs.map(|cert| ((cert.epoch(), *cert.digest()), cert.serializable())),
        )?;
        batch.write()?;
        Ok(())
    }

    pub fn store_effects(
        &self,
        epoch_id: EpochId,
        tx: &TransactionDigest,
        effects: &SignedTransactionEffects,
    ) -> SuiResult {
        Ok(self.pending_effects.insert(&(epoch_id, *tx), effects)?)
    }

    pub fn get_cert_and_effects(
        &self,
        epoch_id: EpochId,
        tx: &TransactionDigest,
    ) -> SuiResult<(
        Option<VerifiedCertificate>,
        Option<SignedTransactionEffects>,
    )> {
        Ok((
            self.pending_certs.get(&(epoch_id, *tx))?.map(|c| c.into()),
            self.pending_effects.get(&(epoch_id, *tx))?,
        ))
    }

    pub fn get_cert(
        &self,
        epoch_id: EpochId,
        tx: &TransactionDigest,
    ) -> SuiResult<Option<VerifiedCertificate>> {
        Ok(self.pending_certs.get(&(epoch_id, *tx))?.map(|c| c.into()))
    }

    pub fn get_effects(
        &self,
        epoch_id: EpochId,
        tx: &TransactionDigest,
    ) -> SuiResult<Option<SignedTransactionEffects>> {
        Ok(self.pending_effects.get(&(epoch_id, *tx))?)
    }

    pub fn cleanup_cert(&self, epoch_id: EpochId, digest: &TransactionDigest) -> SuiResult {
        self.pending_certs.remove(&(epoch_id, *digest))?;
        self.pending_effects.remove(&(epoch_id, *digest))?;
        self.clear_effects_votes(epoch_id, *digest)?;

        Ok(())
    }

    pub fn enqueue_execution_digests(
        &self,
        epoch_id: EpochId,
        peer: AuthorityName,
        seq: TxSequenceNumber,
        digests: &ExecutionDigests,
    ) -> SuiResult {
        let mut write_batch = self.batch_streams.batch();
        trace!(?peer, ?seq, ?digests, "persisting digests to db");
        write_batch = write_batch.insert_batch(
            &self.batch_streams,
            std::iter::once(((epoch_id, peer, seq), digests)),
        )?;

        match self.latest_seq.get(&(epoch_id, peer))? {
            // Note: this can actually happen, because when you request a starting sequence
            // from the validator, it sends you any preceding txes that were in the same
            // batch.
            Some(prev_latest) if prev_latest > seq => (),

            _ => {
                trace!(?peer, ?seq, "recording latest sequence to db");
                write_batch = write_batch
                    .insert_batch(&self.latest_seq, std::iter::once(((epoch_id, peer), seq)))?;
            }
        }

        write_batch.write()?;
        Ok(())
    }

    pub fn batch_stream_iter<'a>(
        &'a self,
        epoch_id: EpochId,
        peer: &'a AuthorityName,
    ) -> SuiResult<impl Iterator<Item = (TxSequenceNumber, ExecutionDigests)> + 'a> {
        Ok(self
            .batch_streams
            .iter()
            .skip_to(&(epoch_id, *peer, 0))?
            .take_while(move |((e, name, _), _)| *e == epoch_id && *name == *peer)
            .map(|((_, _, seq), digests)| (seq, digests)))
    }

    pub fn latest_seq_for_peer(
        &self,
        epoch_id: EpochId,
        peer: &AuthorityName,
    ) -> SuiResult<Option<TxSequenceNumber>> {
        Ok(self.latest_seq.get(&(epoch_id, *peer))?)
    }

    pub fn remove_batch_stream_item(
        &self,
        epoch_id: EpochId,
        peer: AuthorityName,
        seq: TxSequenceNumber,
    ) -> SuiResult {
        Ok(self.batch_streams.remove(&(epoch_id, peer, seq))?)
    }

    pub fn record_effects_vote(
        &self,
        epoch_id: EpochId,
        peer: AuthorityName,
        digest: TransactionDigest,
        effects_digest: TransactionEffectsDigest,
        stake: StakeUnit,
    ) -> SuiResult {
        trace!(?effects_digest, ?peer, ?stake, "recording vote");
        Ok(self
            .effects_votes
            .insert(&(epoch_id, digest, effects_digest, peer), &stake)?)
    }

    fn iter_fx_digest(
        &self,
        epoch_id: EpochId,
        digest: TransactionDigest,
        effects_digest: TransactionEffectsDigest,
    ) -> SuiResult<
        impl Iterator<
                Item = (
                    (TransactionDigest, TransactionEffectsDigest, AuthorityName),
                    StakeUnit,
                ),
            > + '_,
    > {
        Ok(self
            .effects_votes
            .iter()
            .skip_to(&(epoch_id, digest, effects_digest, AuthorityName::ZERO))?
            .take_while(move |((e, tx, fx, _), _)| {
                *e == epoch_id && *tx == digest && *fx == effects_digest
            })
            .map(|((_, tx, fx, peer), vote)| ((tx, fx, peer), vote)))
    }

    pub fn count_effects_votes(
        &self,
        epoch_id: EpochId,
        digest: TransactionDigest,
        effects_digest: TransactionEffectsDigest,
    ) -> SuiResult<StakeUnit> {
        Ok(self
            .iter_fx_digest(epoch_id, digest, effects_digest)?
            .map(|((_, _, _), stake)| stake)
            .sum())
    }

    pub fn get_voters(
        &self,
        epoch_id: EpochId,
        digest: TransactionDigest,
        effects_digest: TransactionEffectsDigest,
    ) -> SuiResult<BTreeSet<AuthorityName>> {
        Ok(self
            .iter_fx_digest(epoch_id, digest, effects_digest)?
            .map(|((_, _, peer), _)| peer)
            .collect())
    }

    pub fn clear_effects_votes(&self, epoch_id: EpochId, digest: TransactionDigest) -> SuiResult {
        trace!(effects_digest = ?digest, "clearing votes");
        Ok(self.effects_votes.multi_remove(
            self.effects_votes
                .iter()
                .skip_to(&(
                    epoch_id,
                    digest,
                    TransactionEffectsDigest::ZERO,
                    AuthorityName::ZERO,
                ))?
                .take_while(move |((e, d, _, _), _)| *e == epoch_id && *d == digest)
                .map(|(k, _)| k),
        )?)
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use sui_types::crypto::{get_authority_key_pair, KeypairTraits};

    #[tokio::test]
    async fn test_stake_votes() {
        let db = NodeSyncStore::new_for_test();

        let epoch_id: EpochId = 1;

        let (_, kp1) = get_authority_key_pair();
        let (_, kp2) = get_authority_key_pair();
        let peer1: AuthorityName = kp1.public().into();
        let peer2: AuthorityName = kp2.public().into();

        let tx1 = TransactionDigest::random();
        let tx2 = TransactionDigest::random();
        let digest1 = TransactionEffectsDigest::random();
        let digest2 = TransactionEffectsDigest::random();

        db.record_effects_vote(epoch_id, peer1, tx1, digest1, 1)
            .unwrap();
        assert_eq!(db.count_effects_votes(epoch_id, tx1, digest1).unwrap(), 1);

        db.record_effects_vote(epoch_id, peer2, tx1, digest1, 2)
            .unwrap();
        assert_eq!(db.count_effects_votes(epoch_id, tx1, digest1).unwrap(), 3);

        assert_eq!(
            db.get_voters(epoch_id, tx1, digest1).unwrap(),
            [peer1, peer2].iter().cloned().collect()
        );

        // redundant votes do not increase total
        db.record_effects_vote(epoch_id, peer2, tx1, digest1, 2)
            .unwrap();
        assert_eq!(db.count_effects_votes(epoch_id, tx1, digest1).unwrap(), 3);

        db.record_effects_vote(epoch_id, peer1, tx2, digest2, 1)
            .unwrap();
        db.record_effects_vote(epoch_id, peer2, tx2, digest2, 2)
            .unwrap();

        db.clear_effects_votes(epoch_id, tx1).unwrap();
        // digest1 is cleared
        assert_eq!(db.count_effects_votes(epoch_id, tx1, digest1).unwrap(), 0);
        // digest2 is not
        assert_eq!(db.count_effects_votes(epoch_id, tx2, digest2).unwrap(), 3);

        // Votes for different effects digests are isolated.
        db.record_effects_vote(epoch_id, peer1, tx1, digest1, 1)
            .unwrap();
        db.record_effects_vote(epoch_id, peer2, tx1, digest2, 2)
            .unwrap();
        assert_eq!(db.count_effects_votes(epoch_id, tx1, digest1).unwrap(), 1);
        assert_eq!(db.count_effects_votes(epoch_id, tx1, digest2).unwrap(), 2);
    }
}
