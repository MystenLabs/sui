use super::*;
use fastx_types::object::Object;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use serde::Serialize;
use std::borrow::Borrow;
use std::path::PathBuf;
use std::sync::Arc;
use typed_store::rocks::DBMap;

// Table keys
const ADDRESS_KEY: &str = "address";
const SECRET_KEY: &str = "secret";

// Column family names
const ADDRESS_CF_NAME: &str = "address";
const SECRET_CF_NAME: &str = "secret";

const CERT_CF_NAME: &str = "certificates";
const SEQ_NUMBER_CF_NAME: &str = "object_sequence_numbers";
const OBJ_REF_CF_NAME: &str = "object_refs";
const TX_DIGEST_TO_CERT_CF_NAME: &str = "object_certs";
const PENDING_ORDERS_CF_NAME: &str = "pending_orders";
const OBJECT_CF_NAME: &str = "objects";

pub fn init_store(path: PathBuf, names: Vec<&str>) -> Arc<DBWithThreadMode<MultiThreaded>> {
    open_cf(&path, None, &names).expect("Cannot open DB.")
}

pub struct ClientStore {
    // These should never change
    pub address: DBMap<String, FastPayAddress>,
    pub secret: DBMap<String, KeyPair>,

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
    /// Map from object ref to actual object to track object history
    /// There can be duplicates and we never delete objects
    pub objects: DBMap<ObjectRef, Object>,
}

impl ClientStore {
    pub fn new(path: PathBuf) -> Self {
        // Open column families
        let db = client_store::init_store(
            path,
            vec![
                ADDRESS_CF_NAME,
                SECRET_CF_NAME,
                PENDING_ORDERS_CF_NAME,
                CERT_CF_NAME,
                SEQ_NUMBER_CF_NAME,
                OBJ_REF_CF_NAME,
                TX_DIGEST_TO_CERT_CF_NAME,
                OBJECT_CF_NAME,
            ],
        );

        ClientStore {
            address: ClientStore::open_db(&db, ADDRESS_CF_NAME),
            secret: ClientStore::open_db(&db, SECRET_CF_NAME),
            pending_orders: ClientStore::open_db(&db, PENDING_ORDERS_CF_NAME),
            certificates: ClientStore::open_db(&db, CERT_CF_NAME),
            object_sequence_numbers: ClientStore::open_db(&db, SEQ_NUMBER_CF_NAME),
            object_refs: ClientStore::open_db(&db, OBJ_REF_CF_NAME),
            object_certs: ClientStore::open_db(&db, TX_DIGEST_TO_CERT_CF_NAME),
            objects: ClientStore::open_db(&db, OBJECT_CF_NAME),
        }
    }

    fn open_db<K, V>(db: &Arc<DBWithThreadMode<MultiThreaded>>, name: &str) -> DBMap<K, V> {
        DBMap::reopen(db, Some(name)).expect(&format!("Cannot open {} CF.", name)[..])
    }
    /// Populate DB with older state
    pub fn populate(
        &self,
        address: FastPayAddress,
        secret: KeyPair,
        object_refs: BTreeMap<ObjectID, ObjectRef>,
        certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
    ) -> Result<(), FastPayError> {
        self.address
            .batch()
            .insert_batch(
                &self.address,
                std::iter::once((ADDRESS_KEY.to_string(), address)),
            )?
            .insert_batch(
                &self.secret,
                std::iter::once((SECRET_KEY.to_string(), secret)),
            )?
            .insert_batch(&self.certificates, certificates.iter())?
            .insert_batch(&self.object_refs, object_refs.iter())?
            .insert_batch(
                &self.object_sequence_numbers,
                object_refs.iter().map(|w| (w.0, w.1 .1)),
            )?
            .write()?;
        Ok(())
    }

    pub fn address(&self) -> Result<Option<FastPayAddress>, FastPayError> {
        self.address
            .get(&ADDRESS_KEY.to_string())
            .map_err(|e| e.into())
    }
    pub fn secret(&self) -> Result<Option<KeyPair>, FastPayError> {
        self.secret
            .get(&SECRET_KEY.to_string())
            .map_err(|e| e.into())
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
