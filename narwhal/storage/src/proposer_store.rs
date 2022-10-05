// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::iter;
use store::rocks::open_cf;
use store::{reopen, rocks::DBMap, Map};
use types::{Header, Round, StoreResult};

/// The storage for the proposer
#[derive(Clone)]
pub struct ProposerStore {
    /// Holds the Last Header that was proposed by the Proposer.
    last_proposed: DBMap<Round, Header>,
}

impl ProposerStore {
    pub fn new(last_proposed: DBMap<Round, Header>) -> ProposerStore {
        Self { last_proposed }
    }

    pub fn new_for_tests() -> ProposerStore {
        const LAST_PROPOSED_CF: &str = "last_proposed";
        let rocksdb = open_cf(tempfile::tempdir().unwrap(), None, &[LAST_PROPOSED_CF])
            .expect("Cannot open database");
        let last_proposed_map = reopen!(&rocksdb, LAST_PROPOSED_CF;<Round, Header>);
        ProposerStore::new(last_proposed_map)
    }

    /// Inserts a proposed header into the store
    pub fn write_last_proposed(&self, round: Round, header: Header) -> StoreResult<()> {
        let mut batch = self.last_proposed.batch();

        // clear the existing headers since we don't need them
        self.last_proposed.clear()?;

        // write the new header
        batch = batch.insert_batch(&self.last_proposed, iter::once((round, header)))?;

        // execute the batch (atomically) and return the result
        batch.write()
    }

    /// Get the last header
    pub fn get_last_header(&self) -> StoreResult<Option<(Round, Header)>> {
        let mut results: Vec<(Round, Header)> = Vec::new();

        let res = self.last_proposed.iter();
        for (round, header) in res {
            results.push((round, header));
        }
        // Return the header corresponding to the largest round.
        // In practice there should only be one entry here.
        results.sort_by_key(|k| k.0);
        let output: Option<(Round, Header)> = match results.len() {
            0 => None,
            n => Some(results[n - 1].clone()),
        };
        Ok(output)
    }
}

#[cfg(test)]
mod test {
    use crate::ProposerStore;
    use store::Map;
    use test_utils::{fixture_batch_with_transactions, CommitteeFixture};
    use types::{CertificateDigest, Header, Round};

    fn create_header_for_round(round: Round) -> Header {
        let builder = types::HeaderBuilder::default();
        let fixture = CommitteeFixture::builder().randomize_ports(true).build();
        let primary = fixture.authorities().next().unwrap();
        let name = primary.public_key();
        let header = builder
            .author(name)
            .round(round)
            .epoch(0)
            .parents([CertificateDigest::default()].iter().cloned().collect())
            .with_payload_batch(fixture_batch_with_transactions(10), 0)
            .build(primary.keypair())
            .unwrap();
        header
    }

    #[tokio::test]
    async fn test_writes() {
        let store = ProposerStore::new_for_tests();
        let header_1 = create_header_for_round(1);
        let round_1: Round = 1;
        let out = store.write_last_proposed(round_1, header_1.clone());
        assert!(out.is_ok());

        let result = store.last_proposed.get(&round_1).unwrap();
        assert_eq!(result.unwrap(), header_1);

        let header_2 = create_header_for_round(2);
        let round_2: Round = 2;
        let out = store.write_last_proposed(round_2, header_2.clone());
        assert!(out.is_ok());

        let should_not_exist = store.last_proposed.get(&round_1).unwrap();
        assert_eq!(should_not_exist, None);

        let should_exist = store.last_proposed.get(&round_2).unwrap();
        assert_eq!(should_exist.unwrap(), header_2);
    }

    #[tokio::test]
    async fn test_reads() {
        let store = ProposerStore::new_for_tests();

        let should_not_exist = store.get_last_header().unwrap();
        assert_eq!(should_not_exist, None);

        let header_1 = create_header_for_round(1);
        let round_1: Round = 1;
        let out = store.write_last_proposed(round_1, header_1.clone());
        assert!(out.is_ok());

        let should_exist = store.get_last_header().unwrap();
        assert_eq!(should_exist.unwrap(), (round_1, header_1));
    }
}
