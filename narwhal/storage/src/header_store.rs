// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, iter, sync::Arc};

use config::AuthorityIdentifier;
use mysten_common::sync::notify_read::NotifyRead;
use store::{rocks::DBMap, Map};
use sui_macros::fail_point;
use types::{HeaderDigest, HeaderKey, Round, SignedHeader};

use crate::StoreResult;

/// Maintains two tables for header storage.
/// One main table which saves the headers by their keys, and a
/// secondary index which speeds up retrieval by author and round.
/// It also offers pub/sub capabilities in write events. By calling
/// `notify_read()`, someone can wait to hear until a header by a specific
/// key has been written in storage.
#[derive(Clone)]
pub struct HeaderStore {
    /// Holds the headers by their key
    header_by_key: DBMap<HeaderKey, SignedHeader>,
    /// A secondary index that keeps the header keys by the authors.
    /// This helps us to perform range requests based on authors. We avoid storing header
    /// here again to not waste space.
    header_key_by_author: DBMap<(AuthorityIdentifier, Round, HeaderDigest), ()>,
    /// The pub/sub to notify for a write that happened for a header key.
    notify_subscribers: Arc<NotifyRead<HeaderKey, SignedHeader>>,
}

impl HeaderStore {
    pub fn new(
        header_by_key: DBMap<HeaderKey, SignedHeader>,
        header_key_by_author: DBMap<(AuthorityIdentifier, Round, HeaderDigest), ()>,
    ) -> HeaderStore {
        Self {
            header_by_key,
            header_key_by_author,
            notify_subscribers: Arc::new(NotifyRead::new()),
        }
    }

    /// Inserts a header to the store
    pub fn write(&self, header: SignedHeader) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        let mut batch = self.header_by_key.batch();

        let key = header.key();

        // write the header by its key
        batch.insert_batch(&self.header_by_key, iter::once((key, header.clone())))?;

        // Index the header key by its author.
        batch.insert_batch(
            &self.header_key_by_author,
            iter::once(((header.author(), header.round(), header.digest()), ())),
        )?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            self.notify_subscribers.notify(&key, &header);
        }

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Inserts multiple headers in the storage. This is an atomic operation.
    /// In the end it notifies any subscribers that are waiting to hear for the
    /// value.
    pub fn write_all(&self, headers: impl IntoIterator<Item = SignedHeader>) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        let mut batch = self.header_by_key.batch();

        let headers: Vec<_> = headers
            .into_iter()
            .map(|header| (header.key(), header))
            .collect();
        let indices = headers
            .iter()
            .map(|(k, _h)| {
                let key = (k.author(), k.round(), k.digest());
                (key, ())
            })
            .collect::<Vec<_>>();

        // write the headers by their keys
        batch.insert_batch(&self.header_by_key, headers.clone())?;

        // write the header keys by their authors
        batch.insert_batch(&self.header_key_by_author, indices)?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            for (_key, header) in &headers {
                self.notify_subscribers.notify(&header.key(), header);
            }
        }

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Retrieves a header from the store. If not found
    /// then None is returned as result.
    pub fn read(&self, key: HeaderKey) -> StoreResult<Option<SignedHeader>> {
        self.header_by_key.get(&key)
    }

    pub fn contains(&self, key: &HeaderKey) -> StoreResult<bool> {
        self.header_by_key.contains_key(key)
    }

    pub fn multi_contains(&self, keys: impl Iterator<Item = HeaderKey>) -> StoreResult<Vec<bool>> {
        self.header_by_key.multi_contains_keys(keys)
    }

    /// Retrieves multiple headers by their provided keys. The results
    /// are returned in the same sequence as the provided keys.
    pub fn read_all(
        &self,
        keys: impl IntoIterator<Item = HeaderKey>,
    ) -> StoreResult<Vec<Option<SignedHeader>>> {
        self.header_by_key.multi_get(keys)
    }

    /// Waits to get notified until the requested header becomes available
    pub async fn notify_read(&self, key: HeaderKey) -> StoreResult<SignedHeader> {
        // we register our interest to be notified with the value
        let receiver = self.notify_subscribers.register_one(&key);

        // let's read the value because we might have missed the opportunity
        // to get notified about it
        if let Ok(Some(header)) = self.read(key) {
            // notify any obligations - and remove the entries
            self.notify_subscribers.notify(&key, &header);

            // reply directly
            return Ok(header);
        }

        // now wait to hear back the result
        let result = receiver.await;

        Ok(result)
    }

    /// Deletes a single header by key.
    pub fn delete(&self, key: HeaderKey) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");
        // first read the header to get the round - we'll need in order
        // to delete the secondary index
        let header = match self.read(key)? {
            Some(header) => header,
            None => return Ok(()),
        };

        let mut batch = self.header_by_key.batch();

        // deletes the header by key
        batch.delete_batch(&self.header_by_key, iter::once(key))?;

        // deletes the header index by author
        let index_key = (header.author(), header.round(), header.digest());
        batch.delete_batch(&self.header_key_by_author, iter::once(index_key))?;

        // execute the batch (atomically) and return the result
        #[allow(clippy::let_and_return)]
        let result = batch.write();

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Deletes multiple headers in an atomic way.
    pub fn delete_all(&self, keys: impl IntoIterator<Item = HeaderKey>) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");
        let keys = keys.into_iter().collect::<Vec<_>>();
        let index_keys = keys
            .iter()
            .map(|key| (key.author(), key.round(), key.digest()))
            .collect::<Vec<_>>();

        let mut batch = self.header_by_key.batch();

        // delete the headers by its keys
        batch.delete_batch(&self.header_by_key, keys.clone())?;

        // delete the header secondary index entries
        batch.delete_batch(&self.header_key_by_author, index_keys)?;

        // execute the batch (atomically) and return the result
        #[allow(clippy::let_and_return)]
        let result = batch.write();

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Retrieves all the headers with round >= the provided round.
    /// The result is returned with headers sorted in round asc order
    pub fn after_round(&self, round: Round) -> StoreResult<Vec<SignedHeader>> {
        // Skip to a row at the requested round.
        let mut iter = self.header_by_key.unbounded_iter();
        if round > 0 {
            let key = HeaderKey::new(
                round,
                AuthorityIdentifier::default(),
                HeaderDigest::default(),
            );
            iter = iter.skip_to(&key)?;
        }

        let mut result = Vec::new();
        for (k, h) in iter {
            match k.round().cmp(&round) {
                Ordering::Equal | Ordering::Greater => {
                    result.push(h);
                }
                Ordering::Less => {
                    continue;
                }
            }
        }

        Ok(result)
    }

    /// Retrieves origins with certificates in each round >= the provided round.
    // pub fn origins_after_round(
    //     &self,
    //     round: Round,
    // ) -> StoreResult<BTreeMap<Round, Vec<AuthorityIdentifier>>> {
    //     // Skip to a row at or before the requested round.
    //     // TODO: Add a more efficient seek method to typed store.
    //     let mut iter = self.certificate_id_by_round.unbounded_iter();
    //     if round > 0 {
    //         iter = iter.skip_to(&(round - 1, AuthorityIdentifier::default()))?;
    //     }

    //     let mut result = BTreeMap::<Round, Vec<AuthorityIdentifier>>::new();
    //     for ((r, origin), _) in iter {
    //         if r < round {
    //             continue;
    //         }
    //         result.entry(r).or_default().push(origin);
    //     }
    //     Ok(result)
    // }

    /// Retrieves the last certificate of the given origin.
    /// Returns None if there is no certificate for the origin.
    // pub fn last_round(&self, origin: AuthorityIdentifier) -> StoreResult<Option<SignedHeader>> {
    //     let key = (origin, Round::MAX);
    //     if let Some(((name, _round), digest)) = self
    //         .header_key_by_author
    //         .unbounded_iter()
    //         .skip_prior_to(&key)?
    //         .next()
    //     {
    //         if name == origin {
    //             return self.read(digest);
    //         }
    //     }
    //     Ok(None)
    // }

    /// Retrieves the highest round number in the store.
    /// Returns 0 if there is no certificate in the store.
    // pub fn highest_round_number(&self) -> Round {
    //     if let Some(((round, _), _)) = self
    //         .certificate_id_by_round
    //         .unbounded_iter()
    //         .skip_to_last()
    //         .reverse()
    //         .next()
    //     {
    //         round
    //     } else {
    //         0
    //     }
    // }

    /// Retrieves the last round number of the given origin.
    /// Returns None if there is no certificate for the origin.
    // pub fn last_round_number(&self, origin: AuthorityIdentifier) -> StoreResult<Option<Round>> {
    //     let key = (origin, Round::MAX);
    //     if let Some(((name, round), _)) = self
    //         .header_key_by_author
    //         .unbounded_iter()
    //         .skip_prior_to(&key)?
    //         .next()
    //     {
    //         if name == origin {
    //             return Ok(Some(round));
    //         }
    //     }
    //     Ok(None)
    // }

    /// Retrieves the next round number bigger than the given round for the origin.
    /// Returns None if there is no more local certificate from the origin with bigger round.
    // pub fn next_round_number(
    //     &self,
    //     origin: AuthorityIdentifier,
    //     round: Round,
    // ) -> StoreResult<Option<Round>> {
    //     let key = (origin, round + 1);
    //     if let Some(((name, round), _)) = self
    //         .header_key_by_author
    //         .unbounded_iter()
    //         .skip_to(&key)?
    //         .next()
    //     {
    //         if name == origin {
    //             return Ok(Some(round));
    //         }
    //     }
    //     Ok(None)
    // }

    /// Clears both the main storage and the secondary index
    pub fn clear(&self) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        self.header_by_key.unsafe_clear()?;
        self.header_key_by_author.unsafe_clear()?;

        fail_point!("narwhal-store-after-write");
        Ok(())
    }

    /// Checks whether the storage is empty. The main storage is
    /// being used to determine this.
    pub fn is_empty(&self) -> bool {
        self.header_by_key.is_empty()
    }
}

#[cfg(test)]
mod test {
    // use crate::header_store::HeaderStore;
    // use config::AuthorityIdentifier;
    // use fastcrypto::hash::Hash;
    // use futures::future::join_all;
    // use std::num::NonZeroUsize;
    // use std::{
    //     collections::{BTreeSet, HashSet},
    //     time::Instant,
    // };
    // use store::rocks::MetricConf;
    // use store::{
    //     reopen,
    //     rocks::{open_cf, DBMap, ReadWriteOptions},
    // };
    // use test_utils::{latest_protocol_version, temp_dir, CommitteeFixture};
    // use types::{
    //     Certificate, CertificateAPI, CertificateDigest, Header, HeaderAPI, HeaderDigest, HeaderKey,
    //     Round,
    // };

    // fn new_store(path: std::path::PathBuf) -> HeaderStore {
    //     let (header_by_key_map, header_key_by_author_map) = create_db_maps(path);
    //     HeaderStore::new(header_by_key_map, header_key_by_author_map)
    // }

    // fn create_db_maps(
    //     path: std::path::PathBuf,
    // ) -> (
    //     DBMap<HeaderKey, SignedHeader>,
    //     DBMap<(AuthorityIdentifier, Round, HeaderDigest), ()>,
    // ) {
    //     const HEADER_BY_KEY_CF: &str = "header_by_key";
    //     const HEADER_KEY_BY_AUTHOR_CF: &str = "header_key_by_author";

    //     let rocksdb = open_cf(
    //         path,
    //         None,
    //         MetricConf::default(),
    //         &[HEADER_BY_KEY_CF, HEADER_KEY_BY_AUTHOR_CF],
    //     )
    //     .expect("Cannot open database");

    //     reopen!(&rocksdb,
    //         HEADER_BY_KEY_CF;<HeaderKey, SignedHeader>,
    //         HEADER_KEY_BY_AUTHOR_CF;<(AuthorityIdentifier, Round, HeaderDigest), ()>
    //     )
    // }

    // helper method that creates certificates for the provided
    // number of rounds.
    // fn certificates(rounds: u64) -> Vec<SignedHeader> {
    //     let fixture = CommitteeFixture::builder().build();
    //     let committee = fixture.committee();
    //     let mut current_round: Vec<_> =
    //         Certificate::genesis(&latest_protocol_version(), &committee)
    //             .into_iter()
    //             .map(|cert| cert.header().clone())
    //             .collect();

    //     let mut result: Vec<SignedHeader> = Vec::new();
    //     for i in 0..rounds {
    //         let parents: BTreeSet<_> = current_round
    //             .iter()
    //             .map(|header| {
    //                 fixture
    //                     .certificate(&latest_protocol_version(), header)
    //                     .digest()
    //             })
    //             .collect();
    //         (_, current_round) = fixture.headers_round(i, &parents, &latest_protocol_version());

    //         result.extend(
    //             current_round
    //                 .iter()
    //                 .map(|h| fixture.certificate(&latest_protocol_version(), h))
    //                 .collect::<Vec<SignedHeader>>(),
    //         );
    //     }

    //     result
    // }

    // #[tokio::test]
    // async fn test_write_and_read() {
    //     test_write_and_read_by_store_type(new_store(temp_dir())).await;
    //     test_write_and_read_by_store_type(new_store_no_cache(temp_dir())).await;
    // }

    // async fn test_write_and_read_by_store_type<T: Cache>(store: HeaderStore<T>) {
    //     // GIVEN
    //     // create certificates for 10 rounds
    //     let certs = certificates(10);
    //     let digests = certs.iter().map(|c| c.digest()).collect::<Vec<_>>();

    //     // verify certs not in the store
    //     for cert in &certs {
    //         assert!(!store.contains(&cert.digest()).unwrap());
    //         assert!(&store.read(cert.digest()).unwrap().is_none());
    //     }

    //     let found = store.multi_contains(digests.iter()).unwrap();
    //     assert_eq!(found.len(), certs.len());
    //     for hit in found {
    //         assert!(!hit);
    //     }

    //     // store the certs
    //     for cert in &certs {
    //         store.write(cert.clone()).unwrap();
    //     }

    //     // verify certs in the store
    //     for cert in &certs {
    //         assert!(store.contains(&cert.digest()).unwrap());
    //         assert_eq!(cert, &store.read(cert.digest()).unwrap().unwrap())
    //     }

    //     let found = store.multi_contains(digests.iter()).unwrap();
    //     assert_eq!(found.len(), certs.len());
    //     for hit in found {
    //         assert!(hit);
    //     }
    // }

    // #[tokio::test]
    // async fn test_write_all_and_read_all() {
    //     test_write_all_and_read_all_by_store_type(new_store(temp_dir())).await;
    //     test_write_all_and_read_all_by_store_type(new_store_no_cache(temp_dir())).await;
    // }

    // async fn test_write_all_and_read_all_by_store_type<T: Cache>(store: HeaderStore<T>) {
    //     // GIVEN
    //     // create certificates for 10 rounds
    //     let certs = certificates(10);
    //     let ids = certs
    //         .iter()
    //         .map(|c| c.digest())
    //         .collect::<Vec<CertificateDigest>>();

    //     // store them in both main and secondary index
    //     store.write_all(certs.clone()).unwrap();

    //     // AND if running with cache, just remove a few items to ensure that they'll be fetched
    //     // from storage
    //     store.cache.remove(&ids[0]);
    //     store.cache.remove(&ids[3]);
    //     store.cache.remove(&ids[9]);

    //     // WHEN
    //     let result = store.read_all(ids).unwrap();

    //     // THEN
    //     assert_eq!(certs.len(), result.len());

    //     for (i, cert) in result.into_iter().enumerate() {
    //         let c = cert.expect("Certificate should have been found");

    //         assert_eq!(&c, certs.get(i).unwrap());
    //     }
    // }

    // #[tokio::test]
    // async fn test_next_round_number() {
    //     // GIVEN
    //     let store = new_store(temp_dir());

    //     // Create certificates for round 1, 2, 4, 6, 9, 10.
    //     let cert = certificates(1).first().unwrap().clone();
    //     let origin = cert.origin();
    //     let rounds = vec![1, 2, 4, 6, 9, 10];
    //     let mut certs = Vec::new();
    //     for r in &rounds {
    //         let mut c = cert.clone();
    //         c.header_mut().update_round(*r);
    //         certs.push(c);
    //     }

    //     store.write_all(certs).unwrap();

    //     // THEN
    //     let mut i = 0;
    //     let mut current_round = 0;
    //     while let Some(r) = store.next_round_number(origin, current_round).unwrap() {
    //         assert_eq!(rounds[i], r);
    //         i += 1;
    //         current_round = r;
    //     }
    // }

    // #[tokio::test]
    // async fn test_last_two_rounds() {
    //     // GIVEN
    //     let store = new_store(temp_dir());

    //     // create certificates for 50 rounds
    //     let certs = certificates(50);
    //     let origin = certs[0].origin();

    //     // store them in both main and secondary index
    //     store.write_all(certs).unwrap();

    //     // WHEN
    //     let result = store.last_two_rounds_certs().unwrap();
    //     let last_round_cert = store.last_round(origin).unwrap().unwrap();
    //     let last_round_number = store.last_round_number(origin).unwrap().unwrap();
    //     let last_round_number_not_exist =
    //         store.last_round_number(AuthorityIdentifier(10u16)).unwrap();
    //     let highest_round_number = store.highest_round_number();

    //     // THEN
    //     assert_eq!(result.len(), 8);
    //     assert_eq!(last_round_cert.round(), 50);
    //     assert_eq!(last_round_number, 50);
    //     assert_eq!(highest_round_number, 50);
    //     for certificate in result {
    //         assert!(
    //             (certificate.round() == last_round_number)
    //                 || (certificate.round() == last_round_number - 1)
    //         );
    //     }
    //     assert!(last_round_number_not_exist.is_none());
    // }

    // #[tokio::test]
    // async fn test_last_round_in_empty_store() {
    //     // GIVEN
    //     let store = new_store(temp_dir());

    //     // WHEN
    //     let result = store.last_two_rounds_certs().unwrap();
    //     let last_round_cert = store.last_round(AuthorityIdentifier::default()).unwrap();
    //     let last_round_number = store
    //         .last_round_number(AuthorityIdentifier::default())
    //         .unwrap();
    //     let highest_round_number = store.highest_round_number();

    //     // THEN
    //     assert!(result.is_empty());
    //     assert!(last_round_cert.is_none());
    //     assert!(last_round_number.is_none());
    //     assert_eq!(highest_round_number, 0);
    // }

    // #[tokio::test]
    // async fn test_after_round() {
    //     // GIVEN
    //     let store = new_store(temp_dir());
    //     let total_rounds = 100;

    //     // create certificates for 50 rounds
    //     let now = Instant::now();

    //     println!("Generating certificates");

    //     let certs = certificates(total_rounds);
    //     println!(
    //         "Created certificates: {} seconds",
    //         now.elapsed().as_secs_f32()
    //     );

    //     let now = Instant::now();
    //     println!("Storing certificates");

    //     // store them in both main and secondary index
    //     store.write_all(certs.clone()).unwrap();

    //     println!(
    //         "Stored certificates: {} seconds",
    //         now.elapsed().as_secs_f32()
    //     );

    //     let round_cutoff = 21;

    //     // now filter the certificates over round 21
    //     let mut certs_ids_over_cutoff_round = certs
    //         .into_iter()
    //         .filter_map(|c| {
    //             if c.round() >= round_cutoff {
    //                 Some(c.digest())
    //             } else {
    //                 None
    //             }
    //         })
    //         .collect::<HashSet<_>>();

    //     // WHEN
    //     println!("Access after round {round_cutoff}, before {total_rounds}");
    //     let now = Instant::now();
    //     let result = store
    //         .after_round(round_cutoff)
    //         .expect("Error returned while reading after_round");

    //     println!("Total time: {} seconds", now.elapsed().as_secs_f32());

    //     // THEN
    //     let certs_per_round = 4;
    //     assert_eq!(
    //         result.len() as u64,
    //         (total_rounds - round_cutoff + 1) * certs_per_round
    //     );

    //     // AND result certificates should be returned in increasing order
    //     let mut last_round = 0;
    //     for certificate in result {
    //         assert!(certificate.round() >= last_round);
    //         last_round = certificate.round();

    //         // should be amongst the certificates of the cut-off round
    //         assert!(certs_ids_over_cutoff_round.remove(&header.key()));
    //     }

    //     // AND none should be left in the original set
    //     assert!(certs_ids_over_cutoff_round.is_empty());

    //     // WHEN get rounds per origin.
    //     let rounds = store
    //         .origins_after_round(round_cutoff)
    //         .expect("Error returned while reading origins_after_round");
    //     assert_eq!(rounds.len(), (total_rounds - round_cutoff + 1) as usize);
    //     for origins in rounds.values() {
    //         assert_eq!(origins.len(), 4);
    //     }
    // }

    // #[tokio::test]
    // async fn test_notify_read() {
    //     let store = new_store(temp_dir());

    //     // run the tests a few times
    //     for _ in 0..10 {
    //         let mut certs = certificates(3);
    //         let mut ids = certs
    //             .iter()
    //             .map(|c| c.digest())
    //             .collect::<Vec<CertificateDigest>>();

    //         let cloned_store = store.clone();

    //         // now populate a certificate
    //         let c1 = certs.remove(0);
    //         store.write(c1.clone()).unwrap();

    //         // spawn a task to notify_read on the certificate's id - we testing
    //         // the scenario where the value is already populated before
    //         // calling the notify read.
    //         let id = ids.remove(0);
    //         let handle_1 = tokio::spawn(async move { cloned_store.notify_read(id).await });

    //         // now spawn a series of tasks before writing anything in store
    //         let mut handles = vec![];
    //         for id in ids {
    //             let cloned_store = store.clone();
    //             let handle = tokio::spawn(async move {
    //                 // wait until the certificate gets populated
    //                 cloned_store.notify_read(id).await
    //             });

    //             handles.push(handle)
    //         }

    //         // and populate the rest with a write_all
    //         store.write_all(certs).unwrap();

    //         // now wait on handle an assert result for a single certificate
    //         let received_certificate = handle_1
    //             .await
    //             .expect("error")
    //             .expect("shouldn't receive store error");

    //         assert_eq!(received_certificate, c1);

    //         let result = join_all(handles).await;
    //         for r in result {
    //             let certificate_result = r.unwrap();
    //             assert!(certificate_result.is_ok());
    //         }

    //         // clear the store before next run
    //         store.clear().unwrap();
    //     }
    // }

    // #[tokio::test]
    // async fn test_write_all_and_clear() {
    //     let store = new_store(temp_dir());

    //     // create certificates for 10 rounds
    //     let certs = certificates(10);

    //     // store them in both main and secondary index
    //     store.write_all(certs).unwrap();

    //     // confirm store is not empty
    //     assert!(!store.is_empty());

    //     // now clear the store
    //     store.clear().unwrap();

    //     // now confirm that store is empty
    //     assert!(store.is_empty());
    // }

    // #[tokio::test]
    // async fn test_delete_by_store_type() {
    //     test_delete(new_store(temp_dir())).await;
    //     test_delete(new_store_no_cache(temp_dir())).await;
    // }

    // async fn test_delete<T: Cache>(store: HeaderStore<T>) {
    //     // GIVEN
    //     // create certificates for 10 rounds
    //     let certs = certificates(10);

    //     // store them in both main and secondary index
    //     store.write_all(certs.clone()).unwrap();

    //     // WHEN now delete a couple of certificates
    //     let to_delete = certs.iter().take(2).map(|c| c.digest()).collect::<Vec<_>>();

    //     store.delete(to_delete[0]).unwrap();
    //     store.delete(to_delete[1]).unwrap();

    //     // THEN
    //     assert!(store.read(to_delete[0]).unwrap().is_none());
    //     assert!(store.read(to_delete[1]).unwrap().is_none());
    // }

    // #[tokio::test]
    // async fn test_delete_all_by_store_type() {
    //     test_delete_all(new_store(temp_dir())).await;
    //     test_delete_all(new_store_no_cache(temp_dir())).await;
    // }

    // async fn test_delete_all<T: Cache>(store: HeaderStore<T>) {
    //     // GIVEN
    //     // create certificates for 10 rounds
    //     let certs = certificates(10);

    //     // store them in both main and secondary index
    //     store.write_all(certs.clone()).unwrap();

    //     // WHEN now delete a couple of certificates
    //     let to_delete = certs.iter().take(2).map(|c| c.digest()).collect::<Vec<_>>();

    //     store.delete_all(to_delete.clone()).unwrap();

    //     // THEN
    //     assert!(store.read(to_delete[0]).unwrap().is_none());
    //     assert!(store.read(to_delete[1]).unwrap().is_none());
    // }
}
