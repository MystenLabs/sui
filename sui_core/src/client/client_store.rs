use super::*;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::path::PathBuf;
use std::sync::Arc;
use sui_types::object::Object;
use typed_store::rocks::DBMap;

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
                PENDING_ORDERS_CF_NAME,
                CERT_CF_NAME,
                SEQ_NUMBER_CF_NAME,
                OBJ_REF_CF_NAME,
                TX_DIGEST_TO_CERT_CF_NAME,
                OBJECT_CF_NAME,
            ],
        );

        ClientStore {
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
        object_refs: BTreeMap<ObjectID, ObjectRef>,
        certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
    ) -> Result<(), SuiError> {
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
}
