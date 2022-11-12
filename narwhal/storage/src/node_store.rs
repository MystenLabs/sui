// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::proposer_store::ProposerKey;
use crate::{CertificateStore, ProposerStore};
use config::WorkerId;
use crypto::PublicKey;
use std::sync::Arc;
use store::rocks::open_cf;
use store::rocks::DBMap;
use store::{reopen, Store};
use types::{
    Batch, BatchDigest, Certificate, CertificateDigest, CommittedSubDagShell, ConsensusStore,
    Header, HeaderDigest, Round, SequenceNumber, VoteInfo,
};

// A type alias marking the "payload" tokens sent by workers to their primary as batch acknowledgements
pub type PayloadToken = u8;

/// All the data stores of the node.
pub struct NodeStorage {
    pub proposer_store: ProposerStore,
    pub vote_digest_store: Store<PublicKey, VoteInfo>,
    pub header_store: Store<HeaderDigest, Header>,
    pub certificate_store: CertificateStore,
    pub payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    pub batch_store: Store<BatchDigest, Batch>,
    pub consensus_store: Arc<ConsensusStore>,
    pub temp_batch_store: Store<(CertificateDigest, BatchDigest), Batch>,
}

impl NodeStorage {
    /// The datastore column family names.
    const LAST_PROPOSED_CF: &'static str = "last_proposed";
    const VOTES_CF: &'static str = "votes";
    const HEADERS_CF: &'static str = "headers";
    const CERTIFICATES_CF: &'static str = "certificates";
    const CERTIFICATE_DIGEST_BY_ROUND_CF: &'static str = "certificate_digest_by_round";
    const CERTIFICATE_DIGEST_BY_ORIGIN_CF: &'static str = "certificate_digest_by_origin";
    const PAYLOAD_CF: &'static str = "payload";
    const BATCHES_CF: &'static str = "batches";
    const LAST_COMMITTED_CF: &'static str = "last_committed";
    const SEQUENCE_CF: &'static str = "sequence";
    const SUB_DAG_CF: &'static str = "sub_dag";
    const TEMP_BATCH_CF: &'static str = "temp_batches";

    /// Open or reopen all the storage of the node.
    pub fn reopen<Path: AsRef<std::path::Path>>(store_path: Path) -> Self {
        let rocksdb = open_cf(
            store_path,
            None,
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
                Self::SEQUENCE_CF,
                Self::SUB_DAG_CF,
                Self::TEMP_BATCH_CF,
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
            sequence_map,
            sub_dag_map,
            temp_batch_map,
        ) = reopen!(&rocksdb,
            Self::LAST_PROPOSED_CF;<ProposerKey, Header>,
            Self::VOTES_CF;<PublicKey, VoteInfo>,
            Self::HEADERS_CF;<HeaderDigest, Header>,
            Self::CERTIFICATES_CF;<CertificateDigest, Certificate>,
            Self::CERTIFICATE_DIGEST_BY_ROUND_CF;<(Round, PublicKey), CertificateDigest>,
            Self::CERTIFICATE_DIGEST_BY_ORIGIN_CF;<(PublicKey, Round), CertificateDigest>,
            Self::PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>,
            Self::BATCHES_CF;<BatchDigest, Batch>,
            Self::LAST_COMMITTED_CF;<PublicKey, Round>,
            Self::SEQUENCE_CF;<SequenceNumber, CertificateDigest>,
            Self::SUB_DAG_CF;<Round, CommittedSubDagShell>,
            Self::TEMP_BATCH_CF;<(CertificateDigest, BatchDigest), Batch>
        );

        let proposer_store = ProposerStore::new(last_proposed_map);
        let vote_digest_store = Store::new(votes_map);
        let header_store = Store::new(header_map);
        let certificate_store = CertificateStore::new(
            certificate_map,
            certificate_digest_by_round_map,
            certificate_digest_by_origin_map,
        );
        let payload_store = Store::new(payload_map);
        let batch_store = Store::new(batch_map);
        let consensus_store = Arc::new(ConsensusStore::new(
            last_committed_map,
            sequence_map,
            sub_dag_map,
        ));
        let temp_batch_store = Store::new(temp_batch_map);

        Self {
            proposer_store,
            vote_digest_store,
            header_store,
            certificate_store,
            payload_store,
            batch_store,
            consensus_store,
            temp_batch_store,
        }
    }
}
