// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::payload_store::PayloadStore;
use crate::proposer_store::ProposerKey;
use crate::vote_digest_store::VoteDigestStore;
use crate::{
    CertificateStore, CertificateStoreCache, CertificateStoreCacheMetrics, ConsensusStore,
    ProposerStore,
};
use config::{AuthorityIdentifier, WorkerId};
use fastcrypto::groups;
use fastcrypto_tbls::nodes::PartyId;
use fastcrypto_tbls::{dkg, dkg_v0};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;
use store::metrics::SamplingInterval;
use store::reopen;
use store::rocks::{default_db_options, open_cf_opts, DBMap, MetricConf, ReadWriteOptions};
use types::{
    Batch, BatchDigest, Certificate, CertificateDigest, CommittedSubDagShell, ConsensusCommit,
    Header, RandomnessRound, Round, SequenceNumber, VoteInfo,
};

// A type alias marking the "payload" tokens sent by workers to their primary as batch acknowledgements
pub type PayloadToken = u8;

// Types used in deprecated random beacon tables.
type PkG = groups::bls12381::G2Element;
type EncG = groups::bls12381::G2Element;

/// All the data stores of the node.
#[derive(Clone)]
pub struct NodeStorage {
    pub proposer_store: ProposerStore,
    pub vote_digest_store: VoteDigestStore,
    pub certificate_store: CertificateStore<CertificateStoreCache>,
    pub payload_store: PayloadStore,
    pub batch_store: DBMap<BatchDigest, Batch>,
    pub consensus_store: Arc<ConsensusStore>,
}

impl NodeStorage {
    /// The datastore column family names.
    pub(crate) const LAST_PROPOSED_CF: &'static str = "last_proposed";
    pub(crate) const VOTES_CF: &'static str = "votes";
    pub(crate) const CERTIFICATES_CF: &'static str = "certificates";
    pub(crate) const CERTIFICATE_DIGEST_BY_ROUND_CF: &'static str = "certificate_digest_by_round";
    pub(crate) const CERTIFICATE_DIGEST_BY_ORIGIN_CF: &'static str = "certificate_digest_by_origin";
    pub(crate) const PAYLOAD_CF: &'static str = "payload";
    pub(crate) const BATCHES_CF: &'static str = "batches";
    pub(crate) const LAST_COMMITTED_CF: &'static str = "last_committed";
    pub(crate) const SUB_DAG_INDEX_CF: &'static str = "sub_dag";
    pub(crate) const COMMITTED_SUB_DAG_INDEX_CF: &'static str = "committed_sub_dag";
    pub(crate) const PROCESSED_MESSAGES_CF: &'static str = "processed_messages";
    pub(crate) const USED_MESSAGES_CF: &'static str = "used_messages";
    pub(crate) const CONFIRMATIONS_CF: &'static str = "confirmations";
    pub(crate) const DKG_OUTPUT_CF: &'static str = "dkg_output";
    pub(crate) const RANDOMNESS_ROUND_CF: &'static str = "randomness_round";

    // 100 nodes * 60 rounds (assuming 1 round/sec this will hold data for about the last 1 minute
    // which should be more than enough for advancing the protocol and also help other nodes)
    // TODO: take into account committee size instead of having fixed 100.
    pub(crate) const CERTIFICATE_STORE_CACHE_SIZE: usize = 100 * 60;

    /// Open or reopen all the storage of the node.
    pub fn reopen<Path: AsRef<std::path::Path> + Send>(
        store_path: Path,
        certificate_store_cache_metrics: Option<Arc<CertificateStoreCacheMetrics>>,
    ) -> Self {
        let db_options = default_db_options().optimize_db_for_write_throughput(2);
        let mut metrics_conf = MetricConf::new("consensus");
        metrics_conf.read_sample_interval = SamplingInterval::new(Duration::from_secs(60), 0);
        let cf_options = db_options.options.clone();
        let column_family_options = vec![
            (Self::LAST_PROPOSED_CF, cf_options.clone()),
            (Self::VOTES_CF, cf_options.clone()),
            (
                Self::CERTIFICATES_CF,
                default_db_options()
                    .optimize_for_write_throughput()
                    .optimize_for_large_values_no_scan(1 << 10)
                    .options,
            ),
            (Self::CERTIFICATE_DIGEST_BY_ROUND_CF, cf_options.clone()),
            (Self::CERTIFICATE_DIGEST_BY_ORIGIN_CF, cf_options.clone()),
            (Self::PAYLOAD_CF, cf_options.clone()),
            (
                Self::BATCHES_CF,
                default_db_options()
                    .optimize_for_write_throughput()
                    .optimize_for_large_values_no_scan(1 << 10)
                    .options,
            ),
            (Self::LAST_COMMITTED_CF, cf_options.clone()),
            (Self::SUB_DAG_INDEX_CF, cf_options.clone()),
            (Self::COMMITTED_SUB_DAG_INDEX_CF, cf_options.clone()),
            (Self::PROCESSED_MESSAGES_CF, cf_options.clone()),
            (Self::USED_MESSAGES_CF, cf_options.clone()),
            (Self::CONFIRMATIONS_CF, cf_options.clone()),
            (Self::DKG_OUTPUT_CF, cf_options.clone()),
            (Self::RANDOMNESS_ROUND_CF, cf_options),
        ];
        let rocksdb = open_cf_opts(
            store_path,
            Some(db_options.options),
            metrics_conf,
            &column_family_options,
        )
        .expect("Cannot open database");

        let (
            last_proposed_map,
            votes_map,
            certificate_map,
            certificate_digest_by_round_map,
            certificate_digest_by_origin_map,
            payload_map,
            batch_map,
            last_committed_map,
            // table `sub_dag` is deprecated in favor of `committed_sub_dag`.
            // This can be removed when DBMap supports removing tables.
            _sub_dag_index_map,
            committed_sub_dag_map,
            // random beacon related tables are deprecated.
            // These can be removed when DBMap supports removing tables.
            _processed_messages_map,
            _used_messages_map,
            _confirmations_map,
            _dkg_output_map,
            _randomness_round_map,
        ) = reopen!(&rocksdb,
            Self::LAST_PROPOSED_CF;<ProposerKey, Header>,
            Self::VOTES_CF;<AuthorityIdentifier, VoteInfo>,
            Self::CERTIFICATES_CF;<CertificateDigest, Certificate>,
            Self::CERTIFICATE_DIGEST_BY_ROUND_CF;<(Round, AuthorityIdentifier), CertificateDigest>,
            Self::CERTIFICATE_DIGEST_BY_ORIGIN_CF;<(AuthorityIdentifier, Round), CertificateDigest>,
            Self::PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>,
            Self::BATCHES_CF;<BatchDigest, Batch>,
            Self::LAST_COMMITTED_CF;<AuthorityIdentifier, Round>,
            Self::SUB_DAG_INDEX_CF;<SequenceNumber, CommittedSubDagShell>,
            Self::COMMITTED_SUB_DAG_INDEX_CF;<SequenceNumber, ConsensusCommit>,
            Self::PROCESSED_MESSAGES_CF;<PartyId, dkg_v0::ProcessedMessage<PkG, EncG>>,
            Self::USED_MESSAGES_CF;<u32, dkg_v0::UsedProcessedMessages<PkG, EncG>>,
            Self::CONFIRMATIONS_CF;<PartyId, dkg::Confirmation<EncG>>,
            Self::DKG_OUTPUT_CF;<u32, dkg::Output<PkG, EncG>>,
            Self::RANDOMNESS_ROUND_CF;<u32, RandomnessRound>
        );

        let proposer_store = ProposerStore::new(last_proposed_map);
        let vote_digest_store = VoteDigestStore::new(votes_map);

        let certificate_store_cache = CertificateStoreCache::new(
            NonZeroUsize::new(Self::CERTIFICATE_STORE_CACHE_SIZE).unwrap(),
            certificate_store_cache_metrics,
        );
        let certificate_store = CertificateStore::<CertificateStoreCache>::new(
            certificate_map,
            certificate_digest_by_round_map,
            certificate_digest_by_origin_map,
            certificate_store_cache,
        );
        let payload_store = PayloadStore::new(payload_map);
        let batch_store = batch_map;
        let consensus_store = Arc::new(ConsensusStore::new(
            last_committed_map,
            committed_sub_dag_map,
        ));

        Self {
            proposer_store,
            vote_digest_store,
            certificate_store,
            payload_store,
            batch_store,
            consensus_store,
        }
    }
}
