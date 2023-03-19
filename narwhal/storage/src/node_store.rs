// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::payload_store::PayloadStore;
use crate::proposer_store::ProposerKey;
use crate::vote_digest_store::VoteDigestStore;
use crate::{CertificateStore, HeaderStore, ProposerStore};
use config::WorkerId;
use crypto::{PublicKey, PublicKeyBytes};
use std::sync::Arc;
use std::time::Duration;
use store::metrics::SamplingInterval;
use store::reopen;
use store::rocks::DBMap;
use store::rocks::{open_cf, MetricConf, ReadWriteOptions};
use types::{
    Batch, BatchDigest, Certificate, CertificateDigest, CommittedSubDagShell, ConsensusStore,
    Header, HeaderDigest, Round, SequenceNumber, VoteInfo,
};

// A type alias marking the "payload" tokens sent by workers to their primary as batch acknowledgements
pub type PayloadToken = u8;

/// All the data stores of the node.
#[derive(Clone)]
pub struct NodeStorage {
    pub proposer_store: ProposerStore,
    pub vote_digest_store: VoteDigestStore,
    pub header_store: HeaderStore,
    pub certificate_store: CertificateStore,
    pub payload_store: PayloadStore,
    pub batch_store: DBMap<BatchDigest, Batch>,
    pub consensus_store: Arc<ConsensusStore>,
}

impl NodeStorage {
    /// The datastore column family names.
    pub(crate) const LAST_PROPOSED_CF: &'static str = "last_proposed";
    pub(crate) const VOTES_CF: &'static str = "votes";
    pub(crate) const HEADERS_CF: &'static str = "headers";
    pub(crate) const CERTIFICATES_CF: &'static str = "certificates";
    pub(crate) const CERTIFICATE_DIGEST_BY_ROUND_CF: &'static str = "certificate_digest_by_round";
    pub(crate) const CERTIFICATE_DIGEST_BY_ORIGIN_CF: &'static str = "certificate_digest_by_origin";
    pub(crate) const PAYLOAD_CF: &'static str = "payload";
    pub(crate) const BATCHES_CF: &'static str = "batches";
    pub(crate) const LAST_COMMITTED_CF: &'static str = "last_committed";
    pub(crate) const SUB_DAG_INDEX_CF: &'static str = "sub_dag";

    /// Open or reopen all the storage of the node.
    pub fn reopen<Path: AsRef<std::path::Path> + Send>(store_path: Path) -> Self {
        let mut metrics_conf = MetricConf::with_db_name("consensus_epoch");
        metrics_conf.read_sample_interval = SamplingInterval::new(Duration::from_secs(60), 0);
        let rocksdb = open_cf(
            store_path,
            None,
            metrics_conf,
            &[
                Self::LAST_PROPOSED_CF,
                Self::VOTES_CF,
                Self::HEADERS_CF,
                Self::CERTIFICATES_CF,
                Self::CERTIFICATE_DIGEST_BY_ROUND_CF,
                Self::CERTIFICATE_DIGEST_BY_ORIGIN_CF,
                Self::PAYLOAD_CF,
                Self::BATCHES_CF,
                Self::LAST_COMMITTED_CF,
                Self::SUB_DAG_INDEX_CF,
            ],
        )
        .expect("Cannot open database");

        let (
            last_proposed_map,
            votes_map,
            header_map,
            certificate_map,
            certificate_digest_by_round_map,
            certificate_digest_by_origin_map,
            payload_map,
            batch_map,
            last_committed_map,
            sub_dag_index_map,
        ) = reopen!(&rocksdb,
            Self::LAST_PROPOSED_CF;<ProposerKey, Header>,
            Self::VOTES_CF;<PublicKey, VoteInfo>,
            Self::HEADERS_CF;<HeaderDigest, Header>,
            Self::CERTIFICATES_CF;<CertificateDigest, Certificate>,
            Self::CERTIFICATE_DIGEST_BY_ROUND_CF;<(Round, PublicKeyBytes), CertificateDigest>,
            Self::CERTIFICATE_DIGEST_BY_ORIGIN_CF;<(PublicKeyBytes, Round), CertificateDigest>,
            Self::PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>,
            Self::BATCHES_CF;<BatchDigest, Batch>,
            Self::LAST_COMMITTED_CF;<PublicKeyBytes, Round>,
            Self::SUB_DAG_INDEX_CF;<SequenceNumber, CommittedSubDagShell>
        );

        let proposer_store = ProposerStore::new(last_proposed_map);
        let vote_digest_store = VoteDigestStore::new(votes_map);
        let header_store = HeaderStore::new(header_map);
        let certificate_store = CertificateStore::new(
            certificate_map,
            certificate_digest_by_round_map,
            certificate_digest_by_origin_map,
        );
        let payload_store = PayloadStore::new(payload_map);
        let batch_store = batch_map;
        let consensus_store = Arc::new(ConsensusStore::new(last_committed_map, sub_dag_index_map));

        Self {
            proposer_store,
            vote_digest_store,
            header_store,
            certificate_store,
            payload_store,
            batch_store,
            consensus_store,
        }
    }
}
