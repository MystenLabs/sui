use super::*;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::borrow::Borrow;
use std::path::PathBuf;
use std::sync::Arc;
use typed_store::rocks::DBMap;
use typed_store::traits::Map;

const CERT_CF_NAME: &str = "certificates";
const SEQ_NUMBER_CF_NAME: &str = "object_sequence_numbers";
const OBJ_REF_CF_NAME: &str = "object_refs";
const TX_DIGEST_TO_CERT_CF_NAME: &str = "object_certs";
const PENDING_ORDERS_CF_NAME: &str = "pending_orders";

pub fn init_store(path: PathBuf, names: Vec<&str>) -> Arc<DBWithThreadMode<MultiThreaded>> {
    open_cf(&path, None, &names).expect("Cannot open DB.")
}

pub struct ClientStore {
    // Table of objects to orders pending on the objects
    pub pending_orders: DBMap<ObjectID, Order>,
    // The remaining fields are used to minimize networking, and may not always be persisted locally.
    /// Known certificates, indexed by TX digest.
    pub certificates: DBMap<TransactionDigest, CertifiedOrder>,
    /// The known objects with it's sequence number owned by the client.
    pub object_sequence_numbers: DBMap<ObjectID, SequenceNumber>,
    /// Confirmed objects with it's ref owned by the client.
    pub object_refs: DBMap<ObjectID, ObjectRef>,
    /// Certificate <-> object id linking map.
    pub object_certs: DBMap<ObjectID, Vec<TransactionDigest>>,
}

impl ClientStore {
    pub fn new(path: PathBuf) -> Self {
        // Open column families
        let db = client_store::init_store(
            path,
            vec![
                PENDING_ORDERS_CF_NAME,
                CERT_CF_NAME,
                SEQ_NUMBER_CF_NAME,
                OBJ_REF_CF_NAME,
                TX_DIGEST_TO_CERT_CF_NAME,
            ],
        );

        ClientStore {
            pending_orders: DBMap::reopen(&db, Some(PENDING_ORDERS_CF_NAME))
                .expect(&format!("Cannot open {} CF.", PENDING_ORDERS_CF_NAME)[..]),
            certificates: DBMap::reopen(&db, Some(CERT_CF_NAME))
                .expect(&format!("Cannot open {} CF.", CERT_CF_NAME)[..]),
            object_sequence_numbers: DBMap::reopen(&db, Some(SEQ_NUMBER_CF_NAME))
                .expect(&format!("Cannot open {} CF.", SEQ_NUMBER_CF_NAME)[..]),
            object_refs: DBMap::reopen(&db, Some(OBJ_REF_CF_NAME))
                .expect(&format!("Cannot open {} CF.", OBJ_REF_CF_NAME)[..]),
            object_certs: DBMap::reopen(&db, Some(TX_DIGEST_TO_CERT_CF_NAME))
                .expect(&format!("Cannot open {} CF.", TX_DIGEST_TO_CERT_CF_NAME)[..]),
        }
    }
    /// Populate DB with older state
    pub fn populate(
        &self,
        object_refs: BTreeMap<ObjectID, ObjectRef>,
        certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
    ) -> Result<(), FastPayError> {
        self.certificates
            .batch()
            .insert_batch(&self.certificates, certificates.iter())?
            .write()?;
        self.object_refs
            .batch()
            .insert_batch(&self.object_refs, object_refs.iter())?
            .write()?;
        self.object_sequence_numbers
            .batch()
            .insert_batch(
                &self.object_sequence_numbers,
                object_refs.iter().map(|w| (w.0, w.1 .1)),
            )?
            .write()?;
        Ok(())
    }

    /// Hack to check if DBMap is empty
    pub fn is_empty<K, V>(map: &DBMap<K, V>) -> bool
    where
        K: Serialize + DeserializeOwned,
        V: Serialize + DeserializeOwned,
    {
        map.iter().next().is_none()
    }

    /// Insert multiple KV pairs atomically
    pub fn multi_insert<J, U, K, V>(
        map: &DBMap<K, V>,
        kv: impl IntoIterator<Item = (J, U)>,
    ) -> Result<(), FastPayError>
    where
        J: Borrow<K>,
        U: Borrow<V>,
        K: Serialize,
        V: Serialize,
    {
        map.batch()
            .insert_batch(map, kv)?
            .write()
            .map_err(|e| e.into())
    }
    /// Remove multiple Keys atomically
    pub fn multi_remove<J, K, V>(
        map: &DBMap<K, V>,
        k: impl IntoIterator<Item = J>,
    ) -> Result<(), FastPayError>
    where
        J: Borrow<K>,
        K: Serialize,
    {
        map.batch()
            .delete_batch(map, k)?
            .write()
            .map_err(|e| e.into())
    }
}
