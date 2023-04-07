// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use fastcrypto::hash::Hash;
use lru::LruCache;
use parking_lot::Mutex;
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::{cmp::Ordering, collections::BTreeMap, iter};
use sui_macros::fail_point;
use tap::Tap;

use crate::StoreResult;
use config::AuthorityIdentifier;
use mysten_common::sync::notify_read::NotifyRead;
use store::{
    rocks::{DBMap, TypedStoreError::RocksDBError},
    Map,
};
use types::{Certificate, CertificateDigest, Round};

#[derive(Clone)]
pub struct CertificateStoreCacheMetrics {
    hit: IntCounter,
    miss: IntCounter,
}

impl CertificateStoreCacheMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            hit: register_int_counter_with_registry!(
                "certificate_store_cache_hit",
                "The number of hits in the cache",
                registry
            )
            .unwrap(),
            miss: register_int_counter_with_registry!(
                "certificate_store_cache_miss",
                "The number of miss in the cache",
                registry
            )
            .unwrap(),
        }
    }
}

/// A cache trait to be used as temporary in-memory store when accessing the underlying
/// certificate_store. Using the cache allows to skip rocksdb access giving us benefits
/// both on less disk access (when value not in db's cache) and also avoiding any additional
/// deserialization costs.
pub trait Cache {
    fn write(&self, certificate: Certificate);
    fn write_all(&self, certificate: Vec<Certificate>);
    fn read(&self, digest: &CertificateDigest) -> Option<Certificate>;

    /// Returns the certificates by performing a look up in the cache. The method is expected to
    /// always return a result for every provided digest (when found will be Some, None otherwise)
    /// and in the same order.
    fn read_all(
        &self,
        digests: Vec<CertificateDigest>,
    ) -> Vec<(CertificateDigest, Option<Certificate>)>;
    fn contains(&self, digest: &CertificateDigest) -> bool;
    fn remove(&self, digest: &CertificateDigest);
    fn remove_all(&self, digests: Vec<CertificateDigest>);
}

/// An LRU cache for the certificate store.
#[derive(Clone)]
pub struct CertificateStoreCache {
    cache: Arc<Mutex<LruCache<CertificateDigest, Certificate>>>,
    metrics: Option<CertificateStoreCacheMetrics>,
}

impl CertificateStoreCache {
    pub fn new(size: NonZeroUsize, metrics: Option<CertificateStoreCacheMetrics>) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(size))),
            metrics,
        }
    }

    fn report_result(&self, is_hit: bool) {
        if let Some(metrics) = self.metrics.as_ref() {
            if is_hit {
                metrics.hit.inc()
            } else {
                metrics.miss.inc()
            }
        }
    }
}

impl Cache for CertificateStoreCache {
    fn write(&self, certificate: Certificate) {
        let mut guard = self.cache.lock();
        guard.put(certificate.digest(), certificate);
    }

    fn write_all(&self, certificate: Vec<Certificate>) {
        let mut guard = self.cache.lock();
        for cert in certificate {
            guard.put(cert.digest(), cert);
        }
    }

    /// Fetches the certificate for the provided digest. This method will update the LRU record
    /// and mark it as "last accessed".
    fn read(&self, digest: &CertificateDigest) -> Option<Certificate> {
        let mut guard = self.cache.lock();
        guard
            .get(digest)
            .cloned()
            .tap(|v| self.report_result(v.is_some()))
    }

    /// Fetches the certificates for the provided digests. This method will update the LRU records
    /// and mark them as "last accessed".
    fn read_all(
        &self,
        digests: Vec<CertificateDigest>,
    ) -> Vec<(CertificateDigest, Option<Certificate>)> {
        let mut guard = self.cache.lock();
        digests
            .into_iter()
            .map(move |id| {
                (
                    id,
                    guard
                        .get(&id)
                        .cloned()
                        .tap(|v| self.report_result(v.is_some())),
                )
            })
            .collect()
    }

    /// Checks whether the value exists in the LRU cache. The method does not update the LRU record, thus
    /// it will not count as a "last access" for the provided digest.
    fn contains(&self, digest: &CertificateDigest) -> bool {
        let guard = self.cache.lock();
        guard
            .contains(digest)
            .tap(|result| self.report_result(*result))
    }

    fn remove(&self, digest: &CertificateDigest) {
        let mut guard = self.cache.lock();
        let _ = guard.pop(digest);
    }

    fn remove_all(&self, digests: Vec<CertificateDigest>) {
        let mut guard = self.cache.lock();
        for digest in digests {
            let _ = guard.pop(&digest);
        }
    }
}

/// An implementation that basically disables the caching functionality when used for CertificateStore.
#[derive(Clone)]
struct NoCache {}

impl Cache for NoCache {
    fn write(&self, _certificate: Certificate) {
        // no-op
    }

    fn write_all(&self, _certificate: Vec<Certificate>) {
        // no-op
    }

    fn read(&self, _digest: &CertificateDigest) -> Option<Certificate> {
        None
    }

    fn read_all(
        &self,
        digests: Vec<CertificateDigest>,
    ) -> Vec<(CertificateDigest, Option<Certificate>)> {
        digests.into_iter().map(|digest| (digest, None)).collect()
    }

    fn contains(&self, _digest: &CertificateDigest) -> bool {
        false
    }

    fn remove(&self, _digest: &CertificateDigest) {
        // no-op
    }

    fn remove_all(&self, _digests: Vec<CertificateDigest>) {
        // no-op
    }
}

/// The main storage when we have to deal with certificates. It maintains
/// two storages, one main which saves the certificates by their ids, and a
/// secondary one which acts as an index to allow us fast retrieval based
/// for queries based in certificate rounds.
/// It also offers pub/sub capabilities in write events. By using the
/// `notify_read` someone can wait to hear until a certificate by a specific
/// id has been written in storage.
#[derive(Clone)]
pub struct CertificateStore<T: Cache = CertificateStoreCache> {
    /// Holds the certificates by their digest id
    certificates_by_id: DBMap<CertificateDigest, Certificate>,
    /// A secondary index that keeps the certificate digest ids
    /// by the certificate rounds. Certificate origin is used to produce unique keys.
    /// This helps us to perform range requests based on rounds. We avoid storing again the
    /// certificate here to not waste space. To dereference we use the certificates_by_id storage.
    certificate_id_by_round: DBMap<(Round, AuthorityIdentifier), CertificateDigest>,
    /// A secondary index that keeps the certificate digest ids
    /// by the certificate origins. Certificate rounds are used to produce unique keys.
    /// This helps us to perform range requests based on rounds. We avoid storing again the
    /// certificate here to not waste space. To dereference we use the certificates_by_id storage.
    certificate_id_by_origin: DBMap<(AuthorityIdentifier, Round), CertificateDigest>,
    /// The pub/sub to notify for a write that happened for a certificate digest id
    notify_subscribers: Arc<NotifyRead<CertificateDigest, Certificate>>,
    /// An LRU cache to keep recent certificates
    cache: Arc<T>,
}

impl<T: Cache> CertificateStore<T> {
    pub fn new(
        certificates_by_id: DBMap<CertificateDigest, Certificate>,
        certificate_id_by_round: DBMap<(Round, AuthorityIdentifier), CertificateDigest>,
        certificate_id_by_origin: DBMap<(AuthorityIdentifier, Round), CertificateDigest>,
        certificate_store_cache: T,
    ) -> CertificateStore<T> {
        Self {
            certificates_by_id,
            certificate_id_by_round,
            certificate_id_by_origin,
            notify_subscribers: Arc::new(NotifyRead::new()),
            cache: Arc::new(certificate_store_cache),
        }
    }

    /// Inserts a certificate to the store
    pub fn write(&self, certificate: Certificate) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        let mut batch = self.certificates_by_id.batch();

        let id = certificate.digest();

        // write the certificate by its id
        batch.insert_batch(
            &self.certificates_by_id,
            iter::once((id, certificate.clone())),
        )?;

        // Index the certificate id by its round and origin.
        batch.insert_batch(
            &self.certificate_id_by_round,
            iter::once(((certificate.round(), certificate.origin()), id)),
        )?;
        batch.insert_batch(
            &self.certificate_id_by_origin,
            iter::once(((certificate.origin(), certificate.round()), id)),
        )?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            self.notify_subscribers.notify(&id, &certificate);
        }

        // insert in cache
        self.cache.write(certificate);

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Inserts multiple certificates in the storage. This is an atomic operation.
    /// In the end it notifies any subscribers that are waiting to hear for the
    /// value.
    pub fn write_all(
        &self,
        certificates: impl IntoIterator<Item = Certificate>,
    ) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        let mut batch = self.certificates_by_id.batch();

        let certificates: Vec<_> = certificates
            .into_iter()
            .map(|certificate| (certificate.digest(), certificate))
            .collect();

        // write the certificates by their ids
        batch.insert_batch(&self.certificates_by_id, certificates.clone())?;

        // write the certificates id by their rounds
        let values = certificates.iter().map(|(digest, c)| {
            let key = (c.round(), c.origin());
            let value = digest;
            (key, value)
        });
        batch.insert_batch(&self.certificate_id_by_round, values)?;

        // write the certificates id by their origins
        let values = certificates.iter().map(|(digest, c)| {
            let key = (c.origin(), c.round());
            let value = digest;
            (key, value)
        });
        batch.insert_batch(&self.certificate_id_by_origin, values)?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            for (_id, certificate) in &certificates {
                self.notify_subscribers
                    .notify(&certificate.digest(), certificate);
            }
        }

        self.cache.write_all(
            certificates
                .into_iter()
                .map(|(_, certificate)| certificate)
                .collect(),
        );

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Retrieves a certificate from the store. If not found
    /// then None is returned as result.
    pub fn read(&self, id: CertificateDigest) -> StoreResult<Option<Certificate>> {
        if let Some(certificate) = self.cache.read(&id) {
            return Ok(Some(certificate));
        }

        self.certificates_by_id.get(&id)
    }

    /// Retrieves a certificate from the store by round and authority.
    /// If not found, None is returned as result.
    pub fn read_by_index(
        &self,
        origin: AuthorityIdentifier,
        round: Round,
    ) -> StoreResult<Option<Certificate>> {
        match self.certificate_id_by_origin.get(&(origin, round))? {
            Some(d) => self.read(d),
            None => Ok(None),
        }
    }

    /// Retrieves a certificate from the store. If not found
    /// then None is returned as result.
    pub fn contains(&self, id: &CertificateDigest) -> StoreResult<bool> {
        if self.cache.contains(id) {
            return Ok(true);
        }

        self.certificates_by_id.contains_key(id)
    }

    /// Retrieves multiple certificates by their provided ids. The results
    /// are returned in the same sequence as the provided keys.
    pub fn read_all(
        &self,
        ids: impl IntoIterator<Item = CertificateDigest>,
    ) -> StoreResult<Vec<Option<Certificate>>> {
        let mut found = HashMap::new();
        let mut missing = Vec::new();

        // first find whatever we can from our local cache
        let ids: Vec<CertificateDigest> = ids.into_iter().collect();
        for (id, certificate) in self.cache.read_all(ids.clone()) {
            if let Some(certificate) = certificate {
                found.insert(id, certificate.clone());
            } else {
                missing.push(id);
            }
        }

        // then fallback for all the misses on the storage
        let from_store = self.certificates_by_id.multi_get(&missing)?;
        from_store
            .iter()
            .zip(missing)
            .for_each(|(certificate, id)| {
                if let Some(certificate) = certificate {
                    found.insert(id, certificate.clone());
                }
            });

        Ok(ids.into_iter().map(|id| found.get(&id).cloned()).collect())
    }

    /// Waits to get notified until the requested certificate becomes available
    pub async fn notify_read(&self, id: CertificateDigest) -> StoreResult<Certificate> {
        // we register our interest to be notified with the value
        let receiver = self.notify_subscribers.register_one(&id);

        // let's read the value because we might have missed the opportunity
        // to get notified about it
        if let Ok(Some(cert)) = self.read(id) {
            // notify any obligations - and remove the entries
            self.notify_subscribers.notify(&id, &cert);

            // reply directly
            return Ok(cert);
        }

        // now wait to hear back the result
        let result = receiver.await;

        Ok(result)
    }

    /// Deletes a single certificate by its digest.
    pub fn delete(&self, id: CertificateDigest) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");
        // first read the certificate to get the round - we'll need in order
        // to delete the secondary index
        let cert = match self.read(id)? {
            Some(cert) => cert,
            None => return Ok(()),
        };

        let mut batch = self.certificates_by_id.batch();

        // write the certificate by its id
        batch.delete_batch(&self.certificates_by_id, iter::once(id))?;

        // write the certificate index by its round
        let key = (cert.round(), cert.origin());

        batch.delete_batch(&self.certificate_id_by_round, iter::once(key))?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            self.cache.remove(&id);
        }

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Deletes multiple certificates in an atomic way.
    pub fn delete_all(&self, ids: impl IntoIterator<Item = CertificateDigest>) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");
        // first read the certificates to get their rounds - we'll need in order
        // to delete the secondary index
        let ids: Vec<CertificateDigest> = ids.into_iter().collect();
        let certs = self.read_all(ids.clone())?;
        let keys_by_round = certs
            .into_iter()
            .filter_map(|c| c.map(|cert| (cert.round(), cert.origin())))
            .collect::<Vec<_>>();
        if keys_by_round.is_empty() {
            return Ok(());
        }

        let mut batch = self.certificates_by_id.batch();

        // delete the certificates from the secondary index
        batch.delete_batch(&self.certificate_id_by_round, keys_by_round)?;

        // delete the certificates by its ids
        batch.delete_batch(&self.certificates_by_id, ids.clone())?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            self.cache.remove_all(ids);
        }

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Retrieves all the certificates with round >= the provided round.
    /// The result is returned with certificates sorted in round asc order
    pub fn after_round(&self, round: Round) -> StoreResult<Vec<Certificate>> {
        // Skip to a row at or before the requested round.
        // TODO: Add a more efficient seek method to typed store.
        let mut iter = self.certificate_id_by_round.iter();
        if round > 0 {
            iter = iter.skip_to(&(round - 1, AuthorityIdentifier::default()))?;
        }

        let mut digests = Vec::new();
        for ((r, _), d) in iter {
            match r.cmp(&round) {
                Ordering::Equal | Ordering::Greater => {
                    digests.push(d);
                }
                Ordering::Less => {
                    continue;
                }
            }
        }

        // Fetch all those certificates from main storage, return an error if any one is missing.
        self.certificates_by_id
            .multi_get(digests.clone())?
            .into_iter()
            .map(|opt_cert| {
                opt_cert.ok_or_else(|| {
                    RocksDBError(format!(
                        "Certificate with some digests not found, CertificateStore invariant violation: {:?}",
                        digests
                    ))
                })
            })
            .collect()
    }

    /// Retrieves origins with certificates in each round >= the provided round.
    pub fn origins_after_round(
        &self,
        round: Round,
    ) -> StoreResult<BTreeMap<Round, Vec<AuthorityIdentifier>>> {
        // Skip to a row at or before the requested round.
        // TODO: Add a more efficient seek method to typed store.
        let mut iter = self.certificate_id_by_round.iter();
        if round > 0 {
            iter = iter.skip_to(&(round - 1, AuthorityIdentifier::default()))?;
        }

        let mut result = BTreeMap::<Round, Vec<AuthorityIdentifier>>::new();
        for ((r, origin), _) in iter {
            if r < round {
                continue;
            }
            result.entry(r).or_default().push(origin);
        }
        Ok(result)
    }

    /// Retrieves the certificates of the last round and the round before that
    pub fn last_two_rounds_certs(&self) -> StoreResult<Vec<Certificate>> {
        // starting from the last element - hence the last round - move backwards until
        // we find certificates of different round.
        let certificates_reverse = self.certificate_id_by_round.iter().skip_to_last().reverse();

        let mut round = 0;
        let mut certificates = Vec::new();

        for (key, digest) in certificates_reverse {
            let (certificate_round, _certificate_origin) = key;

            // We treat zero as special value (round unset) in order to
            // capture the last certificate's round.
            // We are now in a round less than the previous so we want to
            // stop consuming
            if round == 0 {
                round = certificate_round;
            } else if certificate_round < round - 1 {
                break;
            }

            let certificate = self.certificates_by_id.get(&digest)?.ok_or_else(|| {
                RocksDBError(format!(
                    "Certificate with id {} not found in main storage although it should",
                    digest
                ))
            })?;

            certificates.push(certificate);
        }

        Ok(certificates)
    }

    /// Retrieves the last certificate of the given origin.
    /// Returns None if there is no certificate for the origin.
    pub fn last_round(&self, origin: AuthorityIdentifier) -> StoreResult<Option<Certificate>> {
        let key = (origin, Round::MAX);
        if let Some(((name, _round), digest)) = self
            .certificate_id_by_origin
            .iter()
            .skip_prior_to(&key)?
            .next()
        {
            if name == origin {
                return self.read(digest);
            }
        }
        Ok(None)
    }

    /// Retrieves the highest round number in the store.
    /// Returns 0 if there is no certificate in the store.
    pub fn highest_round_number(&self) -> Round {
        if let Some(((round, _), _)) = self
            .certificate_id_by_round
            .iter()
            .skip_to_last()
            .reverse()
            .next()
        {
            round
        } else {
            0
        }
    }

    /// Retrieves the last round number of the given origin.
    /// Returns None if there is no certificate for the origin.
    pub fn last_round_number(&self, origin: AuthorityIdentifier) -> StoreResult<Option<Round>> {
        let key = (origin, Round::MAX);
        if let Some(((name, round), _)) = self
            .certificate_id_by_origin
            .iter()
            .skip_prior_to(&key)?
            .next()
        {
            if name == origin {
                return Ok(Some(round));
            }
        }
        Ok(None)
    }

    /// Retrieves the next round number bigger than the given round for the origin.
    /// Returns None if there is no more local certificate from the origin with bigger round.
    pub fn next_round_number(
        &self,
        origin: AuthorityIdentifier,
        round: Round,
    ) -> StoreResult<Option<Round>> {
        let key = (origin, round + 1);
        if let Some(((name, round), _)) = self.certificate_id_by_origin.iter().skip_to(&key)?.next()
        {
            if name == origin {
                return Ok(Some(round));
            }
        }
        Ok(None)
    }

    /// Clears both the main storage of the certificates and the secondary index
    pub fn clear(&self) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        self.certificates_by_id.clear()?;
        self.certificate_id_by_round.clear()?;
        self.certificate_id_by_origin.clear()?;

        fail_point!("narwhal-store-after-write");
        Ok(())
    }

    /// Checks whether the storage is empty. The main storage is
    /// being used to determine this.
    pub fn is_empty(&self) -> bool {
        self.certificates_by_id.is_empty()
    }
}

#[cfg(test)]
mod test {
    use crate::certificate_store::{CertificateStore, NoCache};
    use crate::{Cache, CertificateStoreCache};
    use config::AuthorityIdentifier;
    use fastcrypto::hash::Hash;
    use futures::future::join_all;
    use std::num::NonZeroUsize;
    use std::{
        collections::{BTreeSet, HashSet},
        time::Instant,
    };
    use store::rocks::MetricConf;
    use store::{
        reopen,
        rocks::{open_cf, DBMap, ReadWriteOptions},
    };
    use test_utils::{temp_dir, CommitteeFixture};
    use types::{Certificate, CertificateAPI, CertificateDigest, HeaderAPI, Round};

    fn new_store(path: std::path::PathBuf) -> CertificateStore {
        let (certificate_map, certificate_id_by_round_map, certificate_id_by_origin_map) =
            create_db_maps(path);

        let store_cache = CertificateStoreCache::new(NonZeroUsize::new(100).unwrap(), None);

        CertificateStore::new(
            certificate_map,
            certificate_id_by_round_map,
            certificate_id_by_origin_map,
            store_cache,
        )
    }

    fn new_store_no_cache(path: std::path::PathBuf) -> CertificateStore<NoCache> {
        let (certificate_map, certificate_id_by_round_map, certificate_id_by_origin_map) =
            create_db_maps(path);

        CertificateStore::new(
            certificate_map,
            certificate_id_by_round_map,
            certificate_id_by_origin_map,
            NoCache {},
        )
    }

    fn create_db_maps(
        path: std::path::PathBuf,
    ) -> (
        DBMap<CertificateDigest, Certificate>,
        DBMap<(Round, AuthorityIdentifier), CertificateDigest>,
        DBMap<(AuthorityIdentifier, Round), CertificateDigest>,
    ) {
        const CERTIFICATES_CF: &str = "certificates";
        const CERTIFICATE_ID_BY_ROUND_CF: &str = "certificate_id_by_round";
        const CERTIFICATE_ID_BY_ORIGIN_CF: &str = "certificate_id_by_origin";

        let rocksdb = open_cf(
            path,
            None,
            MetricConf::default(),
            &[
                CERTIFICATES_CF,
                CERTIFICATE_ID_BY_ROUND_CF,
                CERTIFICATE_ID_BY_ORIGIN_CF,
            ],
        )
        .expect("Cannot open database");

        reopen!(&rocksdb,
            CERTIFICATES_CF;<CertificateDigest, Certificate>,
            CERTIFICATE_ID_BY_ROUND_CF;<(Round, AuthorityIdentifier), CertificateDigest>,
            CERTIFICATE_ID_BY_ORIGIN_CF;<(AuthorityIdentifier, Round), CertificateDigest>
        )
    }

    // helper method that creates certificates for the provided
    // number of rounds.
    fn certificates(rounds: u64) -> Vec<Certificate> {
        let fixture = CommitteeFixture::builder().build();
        let committee = fixture.committee();
        let mut current_round: Vec<_> = Certificate::genesis(&committee)
            .into_iter()
            .map(|cert| cert.header().clone())
            .collect();

        let mut result: Vec<Certificate> = Vec::new();
        for i in 0..rounds {
            let parents: BTreeSet<_> = current_round
                .iter()
                .map(|header| fixture.certificate(header).digest())
                .collect();
            (_, current_round) = fixture.headers_round(i, &parents);

            result.extend(
                current_round
                    .iter()
                    .map(|h| fixture.certificate(h))
                    .collect::<Vec<Certificate>>(),
            );
        }

        result
    }

    #[tokio::test]
    async fn test_write_and_read() {
        test_write_and_read_by_store_type(new_store(temp_dir())).await;
        test_write_and_read_by_store_type(new_store_no_cache(temp_dir())).await;
    }

    async fn test_write_and_read_by_store_type<T: Cache>(store: CertificateStore<T>) {
        // GIVEN
        // create certificates for 10 rounds
        let certs = certificates(10);

        // store them
        for cert in &certs {
            store.write(cert.clone()).unwrap();
        }

        // verify
        for cert in &certs {
            store.contains(&cert.digest()).unwrap();
            assert_eq!(cert, &store.read(cert.digest()).unwrap().unwrap())
        }
    }

    #[tokio::test]
    async fn test_write_all_and_read_all() {
        test_write_all_and_read_all_by_store_type(new_store(temp_dir())).await;
        test_write_all_and_read_all_by_store_type(new_store_no_cache(temp_dir())).await;
    }

    async fn test_write_all_and_read_all_by_store_type<T: Cache>(store: CertificateStore<T>) {
        // GIVEN
        // create certificates for 10 rounds
        let certs = certificates(10);
        let ids = certs
            .iter()
            .map(|c| c.digest())
            .collect::<Vec<CertificateDigest>>();

        // store them in both main and secondary index
        store.write_all(certs.clone()).unwrap();

        // AND if running with cache, just remove a few items to ensure that they'll be fetched
        // from storage
        store.cache.remove(&ids[0]);
        store.cache.remove(&ids[3]);
        store.cache.remove(&ids[9]);

        // WHEN
        let result = store.read_all(ids).unwrap();

        // THEN
        assert_eq!(certs.len(), result.len());

        for (i, cert) in result.into_iter().enumerate() {
            let c = cert.expect("Certificate should have been found");

            assert_eq!(&c, certs.get(i).unwrap());
        }
    }

    #[tokio::test]
    async fn test_next_round_number() {
        // GIVEN
        let store = new_store(temp_dir());

        // Create certificates for round 1, 2, 4, 6, 9, 10.
        let cert = certificates(1).first().unwrap().clone();
        let origin = cert.origin();
        let rounds = vec![1, 2, 4, 6, 9, 10];
        let mut certs = Vec::new();
        for r in &rounds {
            let mut c = cert.clone();
            c.header_mut().update_round(*r);
            certs.push(c);
        }

        store.write_all(certs).unwrap();

        // THEN
        let mut i = 0;
        let mut current_round = 0;
        while let Some(r) = store.next_round_number(origin, current_round).unwrap() {
            assert_eq!(rounds[i], r);
            i += 1;
            current_round = r;
        }
    }

    #[tokio::test]
    async fn test_last_two_rounds() {
        // GIVEN
        let store = new_store(temp_dir());

        // create certificates for 50 rounds
        let certs = certificates(50);
        let origin = certs[0].origin();

        // store them in both main and secondary index
        store.write_all(certs).unwrap();

        // WHEN
        let result = store.last_two_rounds_certs().unwrap();
        let last_round_cert = store.last_round(origin).unwrap().unwrap();
        let last_round_number = store.last_round_number(origin).unwrap().unwrap();
        let last_round_number_not_exist =
            store.last_round_number(AuthorityIdentifier(10u16)).unwrap();
        let highest_round_number = store.highest_round_number();

        // THEN
        assert_eq!(result.len(), 8);
        assert_eq!(last_round_cert.round(), 50);
        assert_eq!(last_round_number, 50);
        assert_eq!(highest_round_number, 50);
        for certificate in result {
            assert!(
                (certificate.round() == last_round_number)
                    || (certificate.round() == last_round_number - 1)
            );
        }
        assert!(last_round_number_not_exist.is_none());
    }

    #[tokio::test]
    async fn test_last_round_in_empty_store() {
        // GIVEN
        let store = new_store(temp_dir());

        // WHEN
        let result = store.last_two_rounds_certs().unwrap();
        let last_round_cert = store.last_round(AuthorityIdentifier::default()).unwrap();
        let last_round_number = store
            .last_round_number(AuthorityIdentifier::default())
            .unwrap();
        let highest_round_number = store.highest_round_number();

        // THEN
        assert!(result.is_empty());
        assert!(last_round_cert.is_none());
        assert!(last_round_number.is_none());
        assert_eq!(highest_round_number, 0);
    }

    #[tokio::test]
    async fn test_after_round() {
        // GIVEN
        let store = new_store(temp_dir());
        let total_rounds = 100;

        // create certificates for 50 rounds
        let now = Instant::now();

        println!("Generating certificates");

        let certs = certificates(total_rounds);
        println!(
            "Created certificates: {} seconds",
            now.elapsed().as_secs_f32()
        );

        let now = Instant::now();
        println!("Storing certificates");

        // store them in both main and secondary index
        store.write_all(certs.clone()).unwrap();

        println!(
            "Stored certificates: {} seconds",
            now.elapsed().as_secs_f32()
        );

        let round_cutoff = 21;

        // now filter the certificates over round 21
        let mut certs_ids_over_cutoff_round = certs
            .into_iter()
            .filter_map(|c| {
                if c.round() >= round_cutoff {
                    Some(c.digest())
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>();

        // WHEN
        println!("Access after round {round_cutoff}, before {total_rounds}");
        let now = Instant::now();
        let result = store
            .after_round(round_cutoff)
            .expect("Error returned while reading after_round");

        println!("Total time: {} seconds", now.elapsed().as_secs_f32());

        // THEN
        let certs_per_round = 4;
        assert_eq!(
            result.len() as u64,
            (total_rounds - round_cutoff + 1) * certs_per_round
        );

        // AND result certificates should be returned in increasing order
        let mut last_round = 0;
        for certificate in result {
            assert!(certificate.round() >= last_round);
            last_round = certificate.round();

            // should be amongst the certificates of the cut-off round
            assert!(certs_ids_over_cutoff_round.remove(&certificate.digest()));
        }

        // AND none should be left in the original set
        assert!(certs_ids_over_cutoff_round.is_empty());

        // WHEN get rounds per origin.
        let rounds = store
            .origins_after_round(round_cutoff)
            .expect("Error returned while reading origins_after_round");
        assert_eq!(rounds.len(), (total_rounds - round_cutoff + 1) as usize);
        for origins in rounds.values() {
            assert_eq!(origins.len(), 4);
        }
    }

    #[tokio::test]
    async fn test_notify_read() {
        let store = new_store(temp_dir());

        // run the tests a few times
        for _ in 0..10 {
            let mut certs = certificates(3);
            let mut ids = certs
                .iter()
                .map(|c| c.digest())
                .collect::<Vec<CertificateDigest>>();

            let cloned_store = store.clone();

            // now populate a certificate
            let c1 = certs.remove(0);
            store.write(c1.clone()).unwrap();

            // spawn a task to notify_read on the certificate's id - we testing
            // the scenario where the value is already populated before
            // calling the notify read.
            let id = ids.remove(0);
            let handle_1 = tokio::spawn(async move { cloned_store.notify_read(id).await });

            // now spawn a series of tasks before writing anything in store
            let mut handles = vec![];
            for id in ids {
                let cloned_store = store.clone();
                let handle = tokio::spawn(async move {
                    // wait until the certificate gets populated
                    cloned_store.notify_read(id).await
                });

                handles.push(handle)
            }

            // and populate the rest with a write_all
            store.write_all(certs).unwrap();

            // now wait on handle an assert result for a single certificate
            let received_certificate = handle_1
                .await
                .expect("error")
                .expect("shouldn't receive store error");

            assert_eq!(received_certificate, c1);

            let result = join_all(handles).await;
            for r in result {
                let certificate_result = r.unwrap();
                assert!(certificate_result.is_ok());
            }

            // clear the store before next run
            store.clear().unwrap();
        }
    }

    #[tokio::test]
    async fn test_write_all_and_clear() {
        let store = new_store(temp_dir());

        // create certificates for 10 rounds
        let certs = certificates(10);

        // store them in both main and secondary index
        store.write_all(certs).unwrap();

        // confirm store is not empty
        assert!(!store.is_empty());

        // now clear the store
        store.clear().unwrap();

        // now confirm that store is empty
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn test_delete_by_store_type() {
        test_delete(new_store(temp_dir())).await;
        test_delete(new_store_no_cache(temp_dir())).await;
    }

    async fn test_delete<T: Cache>(store: CertificateStore<T>) {
        // GIVEN
        // create certificates for 10 rounds
        let certs = certificates(10);

        // store them in both main and secondary index
        store.write_all(certs.clone()).unwrap();

        // WHEN now delete a couple of certificates
        let to_delete = certs.iter().take(2).map(|c| c.digest()).collect::<Vec<_>>();

        store.delete(to_delete[0]).unwrap();
        store.delete(to_delete[1]).unwrap();

        // THEN
        assert!(store.read(to_delete[0]).unwrap().is_none());
        assert!(store.read(to_delete[1]).unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_all_by_store_type() {
        test_delete_all(new_store(temp_dir())).await;
        test_delete_all(new_store_no_cache(temp_dir())).await;
    }

    async fn test_delete_all<T: Cache>(store: CertificateStore<T>) {
        // GIVEN
        // create certificates for 10 rounds
        let certs = certificates(10);

        // store them in both main and secondary index
        store.write_all(certs.clone()).unwrap();

        // WHEN now delete a couple of certificates
        let to_delete = certs.iter().take(2).map(|c| c.digest()).collect::<Vec<_>>();

        store.delete_all(to_delete.clone()).unwrap();

        // THEN
        assert!(store.read(to_delete[0]).unwrap().is_none());
        assert!(store.read(to_delete[1]).unwrap().is_none());
    }

    #[test]
    fn test_cache() {
        // cache should hold up to 5 elements
        let cache = CertificateStoreCache::new(NonZeroUsize::new(5).unwrap(), None);

        let certificates = certificates(5);

        // write 20 certificates
        for cert in &certificates {
            cache.write(cert.clone());
        }

        for (i, cert) in certificates.iter().enumerate() {
            // first 15 certificates should not exist
            if i < 15 {
                assert!(cache.read(&cert.digest()).is_none());
            } else {
                assert!(cache.read(&cert.digest()).is_some());
            }
        }

        // now the same should happen when we use a write_all & read_all
        let cache = CertificateStoreCache::new(NonZeroUsize::new(5).unwrap(), None);

        cache.write_all(certificates.clone());

        let result = cache.read_all(certificates.iter().map(|c| c.digest()).collect());
        for (i, (_, cert)) in result.iter().enumerate() {
            // first 15 certificates should not exist
            if i < 15 {
                assert!(cert.is_none());
            } else {
                assert!(cert.is_some());
            }
        }
    }
}
