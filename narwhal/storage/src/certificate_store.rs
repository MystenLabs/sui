// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crypto::PublicKey;
use dashmap::DashMap;
use fastcrypto::hash::Hash;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, VecDeque},
    iter,
    sync::Arc,
};
use store::{
    rocks::{DBMap, TypedStoreError::RocksDBError},
    Map,
};
use tokio::sync::{oneshot, oneshot::Sender};
use tracing::warn;
use types::{Certificate, CertificateDigest, Round, StoreResult};

/// The main storage when we have to deal with certificates. It maintains
/// two storages, one main which saves the certificates by their ids, and a
/// secondary one which acts as an index to allow us fast retrieval based
/// for queries based in certificate rounds.
/// It also offers pub/sub capabilities in write events. By using the
/// `notify_read` someone can wait to hear until a certificate by a specific
/// id has been written in storage.
#[derive(Clone)]
pub struct CertificateStore {
    /// Holds the certificates by their digest id
    certificates_by_id: DBMap<CertificateDigest, Certificate>,
    /// A secondary index that keeps the certificate digest ids
    /// by the certificate rounds. Certificate origin is used to produce unique keys.
    /// This helps us to perform range requests based on rounds. We avoid storing again the
    /// certificate here to not waste space. To dereference we use the certificates_by_id storage.
    certificate_id_by_round: DBMap<(Round, PublicKey), CertificateDigest>,
    /// A secondary index that keeps the certificate digest ids
    /// by the certificate origins. Certificate rounds are used to produce unique keys.
    /// This helps us to perform range requests based on rounds. We avoid storing again the
    /// certificate here to not waste space. To dereference we use the certificates_by_id storage.
    certificate_id_by_origin: DBMap<(PublicKey, Round), CertificateDigest>,
    /// Senders to notify for a write that happened for
    /// the specified certificate digest id
    notify_on_write_subscribers: Arc<DashMap<CertificateDigest, VecDeque<Sender<Certificate>>>>,
}

impl CertificateStore {
    pub fn new(
        certificates_by_id: DBMap<CertificateDigest, Certificate>,
        certificate_id_by_round: DBMap<(Round, PublicKey), CertificateDigest>,
        certificate_id_by_origin: DBMap<(PublicKey, Round), CertificateDigest>,
    ) -> CertificateStore {
        Self {
            certificates_by_id,
            certificate_id_by_round,
            certificate_id_by_origin,
            notify_on_write_subscribers: Arc::new(DashMap::new()),
        }
    }

    /// Inserts a certificate to the store
    pub fn write(&self, certificate: Certificate) -> StoreResult<()> {
        let mut batch = self.certificates_by_id.batch();

        let id = certificate.digest();

        // write the certificate by its id
        batch = batch.insert_batch(
            &self.certificates_by_id,
            iter::once((id, certificate.clone())),
        )?;

        // Index the certificate id by its round and origin.
        batch = batch.insert_batch(
            &self.certificate_id_by_round,
            iter::once(((certificate.round(), certificate.origin()), id)),
        )?;
        batch = batch.insert_batch(
            &self.certificate_id_by_origin,
            iter::once(((certificate.origin(), certificate.round()), id)),
        )?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            self.notify_subscribers(id, certificate);
        }

        result
    }

    /// Inserts multiple certificates in the storage. This is an atomic operation.
    /// In the end it notifies any subscribers that are waiting to hear for the
    /// value.
    pub fn write_all(
        &self,
        certificates: impl IntoIterator<Item = Certificate>,
    ) -> StoreResult<()> {
        let mut batch = self.certificates_by_id.batch();

        let certificates: Vec<_> = certificates
            .into_iter()
            .map(|certificate| (certificate.digest(), certificate))
            .collect();

        // write the certificates by their ids
        batch = batch.insert_batch(&self.certificates_by_id, certificates.clone())?;

        // write the certificates id by their rounds
        let values = certificates.iter().map(|(digest, c)| {
            let key = (c.round(), c.origin());
            let value = digest;
            (key, value)
        });
        batch = batch.insert_batch(&self.certificate_id_by_round, values)?;

        // write the certificates id by their origins
        let values = certificates.iter().map(|(digest, c)| {
            let key = (c.origin(), c.round());
            let value = digest;
            (key, value)
        });
        batch = batch.insert_batch(&self.certificate_id_by_origin, values)?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            for (_id, certificate) in certificates {
                self.notify_subscribers(certificate.digest(), certificate);
            }
        }

        result
    }

    /// Retrieves a certificate from the store. If not found
    /// then None is returned as result.
    pub fn read(&self, id: CertificateDigest) -> StoreResult<Option<Certificate>> {
        self.certificates_by_id.get(&id)
    }

    /// Retrieves a certificate from the store by round and authority.
    /// If not found, None is returned as result.
    pub fn read_by_index(
        &self,
        origin: PublicKey,
        round: Round,
    ) -> StoreResult<Option<Certificate>> {
        match self.certificate_id_by_origin.get(&(origin, round))? {
            Some(d) => self.certificates_by_id.get(&d),
            None => Ok(None),
        }
    }

    /// Retrieves multiple certificates by their provided ids. The results
    /// are returned in the same sequence as the provided keys.
    pub fn read_all(
        &self,
        ids: impl IntoIterator<Item = CertificateDigest>,
    ) -> StoreResult<Vec<Option<Certificate>>> {
        self.certificates_by_id.multi_get(ids)
    }

    /// Waits to get notified until the requested certificate becomes available
    pub async fn notify_read(&self, id: CertificateDigest) -> StoreResult<Certificate> {
        // we register our interest to be notified with the value
        let (sender, receiver) = oneshot::channel();
        self.notify_on_write_subscribers
            .entry(id)
            .or_insert_with(VecDeque::new)
            .push_back(sender);

        // let's read the value because we might have missed the opportunity
        // to get notified about it
        if let Ok(Some(cert)) = self.read(id) {
            // notify any obligations - and remove the entries
            self.notify_subscribers(id, cert.clone());

            // reply directly
            return Ok(cert);
        }

        // now wait to hear back the result
        let result = receiver
            .await
            .expect("Irrecoverable error while waiting to receive the notify_read result");

        Ok(result)
    }

    /// Deletes a single certificate by its digest.
    pub fn delete(&self, id: CertificateDigest) -> StoreResult<()> {
        // first read the certificate to get the round - we'll need in order
        // to delete the secondary index
        let cert = match self.read(id)? {
            Some(cert) => cert,
            None => return Ok(()),
        };

        let mut batch = self.certificates_by_id.batch();

        // write the certificate by its id
        batch = batch.delete_batch(&self.certificates_by_id, iter::once(id))?;

        // write the certificate index by its round
        let key = (cert.round(), cert.origin());

        batch = batch.delete_batch(&self.certificate_id_by_round, iter::once(key))?;

        // execute the batch (atomically) and return the result
        batch.write()
    }

    /// Deletes multiple certificates in an atomic way.
    pub fn delete_all(&self, ids: impl IntoIterator<Item = CertificateDigest>) -> StoreResult<()> {
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
        batch = batch.delete_batch(&self.certificate_id_by_round, keys_by_round)?;

        // delete the certificates by its ids
        batch = batch.delete_batch(&self.certificates_by_id, ids)?;

        // execute the batch (atomically) and return the result
        batch.write()
    }

    /// Retrieves all the certificates with round >= the provided round.
    /// The result is returned with certificates sorted in round asc order
    pub fn after_round(&self, round: Round) -> StoreResult<Vec<Certificate>> {
        // Skip to a row at or before the requested round.
        // TODO: Add a more efficient seek method to typed store.
        let mut iter = self.certificate_id_by_round.iter();
        if round > 0 {
            iter = iter.skip_to(&(round - 1, PublicKey::default()))?;
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
    ) -> StoreResult<BTreeMap<Round, Vec<PublicKey>>> {
        // Skip to a row at or before the requested round.
        // TODO: Add a more efficient seek method to typed store.
        let mut iter = self.certificate_id_by_round.iter();
        if round > 0 {
            iter = iter.skip_to(&(round - 1, PublicKey::default()))?;
        }

        let mut result = BTreeMap::<Round, Vec<PublicKey>>::new();
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

    /// Retrieves the last round number of the given origin.
    /// Returns None if there is no certificate for the origin.
    pub fn last_round_number(&self, origin: &PublicKey) -> StoreResult<Option<Round>> {
        let key = (origin.clone(), Round::MAX);
        if let Some(((name, round), _)) = self
            .certificate_id_by_origin
            .iter()
            .skip_prior_to(&key)?
            .next()
        {
            if &name == origin {
                return Ok(Some(round));
            }
        }
        Ok(None)
    }

    /// Retrieves the next round number bigger than the given round for the origin.
    /// Returns None if there is no more local certificate from the origin with bigger round.
    pub fn next_round_number(
        &self,
        origin: &PublicKey,
        round: Round,
    ) -> StoreResult<Option<Round>> {
        let key = (origin.clone(), round + 1);
        if let Some(((name, round), _)) = self.certificate_id_by_origin.iter().skip_to(&key)?.next()
        {
            if &name == origin {
                return Ok(Some(round));
            }
        }
        Ok(None)
    }

    /// Clears both the main storage of the certificates and the secondary index
    pub fn clear(&self) -> StoreResult<()> {
        self.certificates_by_id.clear()?;
        self.certificate_id_by_round.clear()?;
        self.certificate_id_by_origin.clear()
    }

    /// Checks whether the storage is empty. The main storage is
    /// being used to determine this.
    pub fn is_empty(&self) -> bool {
        self.certificates_by_id.is_empty()
    }

    /// Notifies the subscribed ones that listen on updates for the
    /// certificate with the provided id. The obligations are notified
    /// with the provided value. The obligation entries under the certificate id
    /// are removed completely. If we fail to notify an obligation we don't
    /// fail and we rather print a warn message.
    fn notify_subscribers(&self, id: CertificateDigest, value: Certificate) {
        if let Some((_, mut senders)) = self.notify_on_write_subscribers.remove(&id) {
            while let Some(s) = senders.pop_front() {
                if s.send(value.clone()).is_err() {
                    warn!("Couldn't notify obligation for certificate with id {id}");
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::certificate_store::CertificateStore;
    use crypto::PublicKey;
    use fastcrypto::hash::Hash;
    use futures::future::join_all;
    use std::{
        collections::{BTreeSet, HashSet},
        time::Instant,
    };
    use store::{
        reopen,
        rocks::{open_cf, DBMap},
    };
    use test_utils::{temp_dir, CommitteeFixture};
    use types::{Certificate, CertificateDigest, Round};

    fn new_store(path: std::path::PathBuf) -> CertificateStore {
        const CERTIFICATES_CF: &str = "certificates";
        const CERTIFICATE_ID_BY_ROUND_CF: &str = "certificate_id_by_round";
        const CERTIFICATE_ID_BY_ORIGIN_CF: &str = "certificate_id_by_origin";

        let rocksdb = open_cf(
            path,
            None,
            &[
                CERTIFICATES_CF,
                CERTIFICATE_ID_BY_ROUND_CF,
                CERTIFICATE_ID_BY_ORIGIN_CF,
            ],
        )
        .expect("Cannot open database");

        let (certificate_map, certificate_id_by_round_map, certificate_id_by_origin_map) = reopen!(&rocksdb,
            CERTIFICATES_CF;<CertificateDigest, Certificate>,
            CERTIFICATE_ID_BY_ROUND_CF;<(Round, PublicKey), CertificateDigest>,
            CERTIFICATE_ID_BY_ORIGIN_CF;<(PublicKey, Round), CertificateDigest>
        );

        CertificateStore::new(
            certificate_map,
            certificate_id_by_round_map,
            certificate_id_by_origin_map,
        )
    }

    // helper method that creates certificates for the provided
    // number of rounds.
    fn certificates(rounds: u64) -> Vec<Certificate> {
        let fixture = CommitteeFixture::builder().build();
        let committee = fixture.committee();
        let mut current_round: Vec<_> = Certificate::genesis(&committee)
            .into_iter()
            .map(|cert| cert.header)
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
    async fn test_write_all_and_read_all() {
        // GIVEN
        let store = new_store(temp_dir());

        // create certificates for 10 rounds
        let certs = certificates(10);
        let ids = certs
            .iter()
            .map(|c| c.digest())
            .collect::<Vec<CertificateDigest>>();

        // store them in both main and secondary index
        store.write_all(certs.clone()).unwrap();

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
            c.header.round = *r as u64;
            certs.push(c);
        }

        store.write_all(certs).unwrap();

        // THEN
        let mut i = 0;
        let mut current_round = 0;
        while let Some(r) = store.next_round_number(&origin, current_round).unwrap() {
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
        let last_round_number = store.last_round_number(&origin).unwrap().unwrap();
        let last_round_number_not_exist = store.last_round_number(&PublicKey::default()).unwrap();

        // THEN
        assert_eq!(result.len(), 8);
        assert_eq!(last_round_number, 50);
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
        let last_round_number = store.last_round_number(&PublicKey::default()).unwrap();

        // THEN
        assert!(result.is_empty());
        assert!(last_round_number.is_none());
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

            // now wait on handle an assert result for a signle certificate
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
    async fn test_delete() {
        // GIVEN
        let store = new_store(temp_dir());

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
    async fn test_delete_all() {
        // GIVEN
        let store = new_store(temp_dir());

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
}
