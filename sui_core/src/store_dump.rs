use narwhal_executor::ExecutionIndices;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::batch::{SignedBatch, TxSequenceNumber};
use sui_types::crypto::{AuthoritySignInfo, EmptySignInfo};
use sui_types::messages::{CertifiedTransaction, TransactionEffectsEnvelope, TransactionEnvelope};
use sui_types::object::Object;
use typed_store::rocks::DBMap;
use typed_store::{reopen, traits::Map};

pub type AuthorityStoreReadOnly = SuiDataStoreReadonly<AuthoritySignInfo>;
pub type GatewayStoreReadOnly = SuiDataStoreReadonly<EmptySignInfo>;

const OBJECTS_TABLE_NAME: &str = "objects";
const ALL_OBJ_VER_TABLE_NAME: &str = "all_object_versions";
const OWNER_INDEX_TABLE_NAME: &str = "owner_index";
const TX_LOCK_TABLE_NAME: &str = "transaction_lock";
const TX_TABLE_NAME: &str = "transactions";
const CERTS_TABLE_NAME: &str = "certificates";
const PARENT_SYNC_TABLE_NAME: &str = "parent_sync";
const EFFECTS_TABLE_NAME: &str = "effects";
const SEQUENCED_TABLE_NAME: &str = "sequenced";
const SCHEDULE_TABLE_NAME: &str = "schedule";
const BATCHES_TABLE_NAME: &str = "batches";
const EXEC_SEQ_TABLE_NAME: &str = "executed_sequence";
const LAST_CONSENSUS_TABLE_NAME: &str = "last_consensus_index";

/// S is a template on Authority signature state. This allows SuiDataStore to be used on either
/// authorities or non-authorities. Specifically, when storing transactions and effects,
/// S allows SuiDataStore to either store the authority signed version or unsigned version.
pub struct SuiDataStoreReadonly<S> {
    /// This is a map between the object ID and the latest state of the object, namely the
    /// state that is needed to process new transactions. If an object is deleted its entry is
    /// removed from this map.
    objects: DBMap<ObjectID, Object>,

    /// Stores all history versions of all objects.
    /// This is not needed by an authority, but is needed by a replica.
    #[allow(dead_code)]
    all_object_versions: DBMap<(ObjectID, SequenceNumber), Object>,

    /// This is a map between object references of currently active objects that can be mutated,
    /// and the transaction that they are lock on for use by this specific authority. Where an object
    /// lock exists for an object version, but no transaction has been seen using it the lock is set
    /// to None. The safety of consistent broadcast depend on each honest authority never changing
    /// the lock once it is set. After a certificate for this object is processed it can be
    /// forgotten.
    transaction_lock: DBMap<ObjectRef, Option<TransactionDigest>>,

    /// This is a an index of object references to currently existing objects, indexed by the
    /// composite key of the SuiAddress of their owner and the object ID of the object.
    /// This composite index allows an efficient iterator to list all objected currently owned
    /// by a specific user, and their object reference.
    owner_index: DBMap<(SuiAddress, ObjectID), ObjectRef>,

    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    /// NOTE: after a lock is deleted (after a certificate is processed) the corresponding entry here
    /// could be deleted, but right now this is only done on gateways, not done on authorities.
    transactions: DBMap<TransactionDigest, TransactionEnvelope<S>>,

    /// This is a map between the transaction digest and the corresponding certificate for all
    /// certificates that have been successfully processed by this authority. This set of certificates
    /// along with the genesis allows the reconstruction of all other state, and a full sync to this
    /// authority.
    certificates: DBMap<TransactionDigest, CertifiedTransaction>,

    /// The map between the object ref of objects processed at all versions and the transaction
    /// digest of the certificate that lead to the creation of this version of the object.
    ///
    /// When an object is deleted we include an entry into this table for its next version and
    /// a digest of ObjectDigest::deleted(), along with a link to the transaction that deleted it.
    parent_sync: DBMap<ObjectRef, TransactionDigest>,

    /// A map between the transaction digest of a certificate that was successfully processed
    /// (ie in `certificates`) and the effects its execution has on the authority state. This
    /// structure is used to ensure we do not double process a certificate, and that we can return
    /// the same response for any call after the first (ie. make certificate processing idempotent).
    effects: DBMap<TransactionDigest, TransactionEffectsEnvelope<S>>,

    /// Hold the lock for shared objects. These locks are written by a single task: upon receiving a valid
    /// certified transaction from consensus, the authority assigns a lock to each shared objects of the
    /// transaction. Note that all authorities are guaranteed to assign the same lock to these objects.
    /// TODO: These two maps should be merged into a single one (no reason to have two).
    sequenced: DBMap<(TransactionDigest, ObjectID), SequenceNumber>,
    schedule: DBMap<ObjectID, SequenceNumber>,

    // Tables used for authority batch structure
    /// A sequence on all executed certificates and effects.
    pub executed_sequence: DBMap<TxSequenceNumber, TransactionDigest>,

    /// A sequence of batches indexing into the sequence of executed transactions.
    pub batches: DBMap<TxSequenceNumber, SignedBatch>,

    /// The following table is used to store a single value (the corresponding key is a constant). The value
    /// represents the index of the latest consensus message this authority processed. This field is written
    /// by a single process acting as consensus (light) client. It is used to ensure the authority processes
    /// every message output by consensus (and in the right order).
    last_consensus_index: DBMap<u64, ExecutionIndices>,
}

impl<S: Eq + Serialize + for<'de> Deserialize<'de>> SuiDataStoreReadonly<S> {
    /// Open an authority store by directory path
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> Self {
        let mut options = db_options.unwrap_or_default();

        // One common issue when running tests on Mac is that the default ulimit is too low,
        // leading to I/O errors such as "Too many open files". Raising fdlimit to bypass it.
        options.set_max_open_files(-1);

        /* The table cache is locked for updates and this determines the number
           of shareds, ie 2^10. Increase in case of lock contentions.
        */
        let row_cache = rocksdb::Cache::new_lru_cache(1_000_000).expect("Cache is ok");
        options.set_row_cache(&row_cache);
        options.set_table_cache_num_shard_bits(10);
        options.set_compression_type(rocksdb::DBCompressionType::None);

        let mut point_lookup = options.clone();
        point_lookup.optimize_for_point_lookup(1024 * 1024);
        point_lookup.set_memtable_whole_key_filtering(true);

        let transform = rocksdb::SliceTransform::create("bytes_8_to_16", |key| &key[8..16], None);
        point_lookup.set_prefix_extractor(transform);
        point_lookup.set_memtable_prefix_bloom_ratio(0.2);

        let db = {
            let path = &path;
            let db_options = Some(options.clone());
            let opt_cfs: &[(&str, &rocksdb::Options)] = &[
                (OBJECTS_TABLE_NAME, &point_lookup),
                (ALL_OBJ_VER_TABLE_NAME, &options),
                (TX_TABLE_NAME, &point_lookup),
                (OWNER_INDEX_TABLE_NAME, &options),
                (TX_LOCK_TABLE_NAME, &point_lookup),
                (CERTS_TABLE_NAME, &point_lookup),
                (PARENT_SYNC_TABLE_NAME, &options),
                (EFFECTS_TABLE_NAME, &point_lookup),
                (SEQUENCED_TABLE_NAME, &options),
                (SCHEDULE_TABLE_NAME, &options),
                (EXEC_SEQ_TABLE_NAME, &options),
                (BATCHES_TABLE_NAME, &options),
                (LAST_CONSENSUS_TABLE_NAME, &options),
            ];
            typed_store::rocks::open_cf_opts_secondary(path, db_options, opt_cfs)
        }
        .expect("Cannot open DB.");

        let executed_sequence =
            DBMap::reopen(&db, Some(EXEC_SEQ_TABLE_NAME)).expect("Cannot open CF.");

        let (
            objects,
            all_object_versions,
            owner_index,
            transaction_lock,
            transactions,
            certificates,
            parent_sync,
            effects,
            sequenced,
            schedule,
            batches,
            last_consensus_index,
        ) = reopen! (
            &db,
            OBJECTS_TABLE_NAME;<ObjectID, Object>,
            ALL_OBJ_VER_TABLE_NAME;<(ObjectID, SequenceNumber), Object>,
            OWNER_INDEX_TABLE_NAME;<(SuiAddress, ObjectID), ObjectRef>,
            TX_LOCK_TABLE_NAME;<ObjectRef, Option<TransactionDigest>>,
            TX_TABLE_NAME;<TransactionDigest, TransactionEnvelope<S>>,
            CERTS_TABLE_NAME;<TransactionDigest, CertifiedTransaction>,
            PARENT_SYNC_TABLE_NAME;<ObjectRef, TransactionDigest>,
            EFFECTS_TABLE_NAME;<TransactionDigest, TransactionEffectsEnvelope<S>>,
            SEQUENCED_TABLE_NAME;<(TransactionDigest, ObjectID), SequenceNumber>,
            SCHEDULE_TABLE_NAME;<ObjectID, SequenceNumber>,
            BATCHES_TABLE_NAME;<TxSequenceNumber, SignedBatch>,
            LAST_CONSENSUS_TABLE_NAME;<u64, ExecutionIndices>
        );
        Self {
            objects,
            all_object_versions,
            owner_index,
            transaction_lock,
            transactions,
            certificates,
            parent_sync,
            effects,
            sequenced,
            schedule,
            executed_sequence,
            batches,
            last_consensus_index,
        }
    }
}

fn dump<Q>(store: SuiDataStoreReadonly<Q>, table_name: &str) -> BTreeMap<String, String>
where
    Q: Eq + Serialize + for<'de> Deserialize<'de> + Debug,
{
    match table_name {
        OBJECTS_TABLE_NAME => store
            .objects
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        ALL_OBJ_VER_TABLE_NAME => store
            .all_object_versions
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        OWNER_INDEX_TABLE_NAME => store
            .owner_index
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        TX_LOCK_TABLE_NAME => store
            .transaction_lock
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        TX_TABLE_NAME => store
            .transactions
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        CERTS_TABLE_NAME => store
            .certificates
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        PARENT_SYNC_TABLE_NAME => store
            .parent_sync
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        EFFECTS_TABLE_NAME => store
            .effects
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        SEQUENCED_TABLE_NAME => store
            .sequenced
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        SCHEDULE_TABLE_NAME => store
            .schedule
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        BATCHES_TABLE_NAME => store
            .batches
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        LAST_CONSENSUS_TABLE_NAME => store
            .last_consensus_index
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        EXEC_SEQ_TABLE_NAME => store
            .executed_sequence
            .iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect::<BTreeMap<_, _>>(),
        _ => panic!("No such table name"),
    }
}

pub fn dump_table(gateway: bool, path: PathBuf, table_name: &str) -> BTreeMap<String, String> {
    if gateway {
        dump(GatewayStoreReadOnly::open(path, None), table_name)
    } else {
        dump(AuthorityStoreReadOnly::open(path, None), table_name)
    }
}
