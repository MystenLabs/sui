// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::StoreResult;
use store::rocks::{open_cf, MetricConf};
use store::{reopen, rocks::DBMap, rocks::ReadWriteOptions, Map};
use sui_macros::fail_point;
use types::Header;

pub type ProposerKey = u32;

pub const LAST_PROPOSAL_KEY: ProposerKey = 0;

/// The storage for the proposer
#[derive(Clone)]
pub struct ProposerStore {
    /// Holds the Last Header that was proposed by the Proposer.
    last_proposed: DBMap<ProposerKey, Header>,
}

impl ProposerStore {
    pub fn new(last_proposed: DBMap<ProposerKey, Header>) -> ProposerStore {
        Self { last_proposed }
    }

    pub fn new_for_tests() -> ProposerStore {
        const LAST_PROPOSED_CF: &str = "last_proposed";
        let rocksdb = open_cf(
            tempfile::tempdir().unwrap(),
            None,
            MetricConf::default(),
            &[LAST_PROPOSED_CF],
        )
        .expect("Cannot open database");
        let last_proposed_map = reopen!(&rocksdb, LAST_PROPOSED_CF;<ProposerKey, Header>);
        ProposerStore::new(last_proposed_map)
    }

    /// Inserts a proposed header into the store
    #[allow(clippy::let_and_return)]
    pub fn write_last_proposed(&self, header: &Header) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        let result = self.last_proposed.insert(&LAST_PROPOSAL_KEY, header);

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Get the last header
    pub fn get_last_proposed(&self) -> StoreResult<Option<Header>> {
        self.last_proposed.get(&LAST_PROPOSAL_KEY)
    }
}

#[cfg(test)]
mod test {
    use crate::{ProposerStore, LAST_PROPOSAL_KEY};
    use store::Map;
    use test_utils::{fixture_batch_with_transactions, latest_protocol_version, CommitteeFixture};
    use types::{CertificateDigest, Header, Round};

    pub fn create_header_for_round(round: Round) -> Header {
        let builder = types::HeaderV1Builder::default();
        let fixture = CommitteeFixture::builder().randomize_ports(true).build();
        let primary = fixture.authorities().next().unwrap();
        let id = primary.id();
        let header = builder
            .author(id)
            .round(round)
            .epoch(fixture.committee().epoch())
            .parents([CertificateDigest::default()].iter().cloned().collect())
            .with_payload_batch(
                fixture_batch_with_transactions(10, &latest_protocol_version()),
                0,
                0,
            )
            .build()
            .unwrap();
        Header::V1(header)
    }

    #[tokio::test]
    async fn test_writes() {
        let store = ProposerStore::new_for_tests();
        let header_1 = create_header_for_round(1);

        let out = store.write_last_proposed(&header_1);
        assert!(out.is_ok());

        let result = store.last_proposed.get(&LAST_PROPOSAL_KEY).unwrap();
        assert_eq!(result.unwrap(), header_1);

        let header_2 = create_header_for_round(2);
        let out = store.write_last_proposed(&header_2);
        assert!(out.is_ok());

        let should_exist = store.last_proposed.get(&LAST_PROPOSAL_KEY).unwrap();
        assert_eq!(should_exist.unwrap(), header_2);
    }

    #[tokio::test]
    async fn test_reads() {
        let store = ProposerStore::new_for_tests();

        let should_not_exist = store.get_last_proposed().unwrap();
        assert_eq!(should_not_exist, None);

        let header_1 = create_header_for_round(1);
        let out = store.write_last_proposed(&header_1);
        assert!(out.is_ok());

        let should_exist = store.get_last_proposed().unwrap();
        assert_eq!(should_exist.unwrap(), header_1);
    }
}
