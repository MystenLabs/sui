// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use narwhal_executor::ExecutionIndices;

use rocksdb::Options;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use sui_core::epoch::EpochInfoLocals;
use sui_storage::default_db_options;
use sui_types::base_types::{
    ExecutionDigests, ObjectID, ObjectInfo, ObjectRef, SequenceNumber, TransactionDigest,
    VersionNumber,
};
use sui_types::batch::{SignedBatch, TxSequenceNumber};
use sui_types::committee::EpochId;
use sui_types::crypto::{AuthoritySignInfo, EmptySignInfo};
use sui_types::messages::{CertifiedTransaction, TransactionEffectsEnvelope, TransactionEnvelope};
use sui_types::object::Object;
use sui_types::object::Owner;
use typed_store::rocks::DBMap;
use typed_store::{reopen, traits::Map};

pub type InternalSequenceNumber = u64;
pub type AuthorityStoreReadOnly = SuiDataStoreReadonly<AuthoritySignInfo>;
pub type GatewayStoreReadOnly = SuiDataStoreReadonly<EmptySignInfo>;

#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Copy, PartialOrd, Ord, Hash, Serialize, Deserialize)]
struct ObjectKey(pub ObjectID, pub VersionNumber);

const OBJECTS_TABLE_NAME: &str = "objects";
const OWNER_INDEX_TABLE_NAME: &str = "owner_index";
const TX_TABLE_NAME: &str = "transactions";
const CERTS_TABLE_NAME: &str = "certificates";
const PENDING_EXECUTION: &str = "pending_execution";
const PARENT_SYNC_TABLE_NAME: &str = "parent_sync";
const EFFECTS_TABLE_NAME: &str = "effects";
const SEQUENCED_TABLE_NAME: &str = "sequenced";
const SCHEDULE_TABLE_NAME: &str = "schedule";
const EXEC_SEQ_TABLE_NAME: &str = "executed_sequence";
const BATCHES_TABLE_NAME: &str = "batches";
const LAST_CONSENSUS_TABLE_NAME: &str = "last_consensus_index";
const EPOCH_TABLE_NAME: &str = "epochs";

/// S is a template on Authority signature state. This allows SuiDataStore to be used on either
/// authorities or non-authorities. Specifically, when storing transactions and effects,
/// S allows SuiDataStore to either store the authority signed version or unsigned version.
pub struct SuiDataStoreReadonly<S> {
    objects: DBMap<ObjectKey, Object>,

    owner_index: DBMap<(Owner, ObjectID), ObjectInfo>,

    transactions: DBMap<TransactionDigest, TransactionEnvelope<S>>,

    certificates: DBMap<TransactionDigest, CertifiedTransaction>,

    pending_execution: DBMap<InternalSequenceNumber, TransactionDigest>,

    parent_sync: DBMap<ObjectRef, TransactionDigest>,

    effects: DBMap<TransactionDigest, TransactionEffectsEnvelope<S>>,

    sequenced: DBMap<(TransactionDigest, ObjectID), SequenceNumber>,
    schedule: DBMap<ObjectID, SequenceNumber>,

    executed_sequence: DBMap<TxSequenceNumber, ExecutionDigests>,

    batches: DBMap<TxSequenceNumber, SignedBatch>,

    last_consensus_index: DBMap<u64, ExecutionIndices>,

    epochs: DBMap<EpochId, EpochInfoLocals>,
}

impl<S: Eq + Serialize + for<'de> Deserialize<'de>> SuiDataStoreReadonly<S> {
    /// Open an authority store by directory path
    pub fn open<P: AsRef<Path>>(
        primary_path: P,
        secondary_path: P,
        db_options: Option<Options>,
    ) -> Self {
        let (options, point_lookup) = default_db_options(db_options, None);

        let db = {
            let path = &primary_path;
            let s_path = &secondary_path;
            let db_options = Some(options.clone());
            let opt_cfs: &[(&str, &rocksdb::Options)] = &[
                ("objects", &point_lookup),
                ("transactions", &point_lookup),
                ("owner_index", &options),
                ("certificates", &point_lookup),
                ("pending_execution", &options),
                ("parent_sync", &options),
                ("effects", &point_lookup),
                ("sequenced", &options),
                ("schedule", &options),
                ("executed_sequence", &options),
                ("batches", &options),
                ("last_consensus_index", &options),
                ("epochs", &options),
            ];

            typed_store::rocks::open_cf_opts_secondary(path, Some(s_path), db_options, opt_cfs)
        }
        .expect("Cannot open DB.");

        let executed_sequence =
            DBMap::reopen(&db, Some("executed_sequence")).expect("Cannot open CF.");

        let (
            objects,
            owner_index,
            transactions,
            certificates,
            pending_execution,
            parent_sync,
            effects,
            sequenced,
            schedule,
            batches,
            last_consensus_index,
            epochs,
        ) = reopen! (
            &db,
            "objects";<ObjectKey, Object>,
            "owner_index";<(Owner, ObjectID), ObjectInfo>,
            "transactions";<TransactionDigest, TransactionEnvelope<S>>,
            "certificates";<TransactionDigest, CertifiedTransaction>,
            "pending_execution";<InternalSequenceNumber, TransactionDigest>,
            "parent_sync";<ObjectRef, TransactionDigest>,
            "effects";<TransactionDigest, TransactionEffectsEnvelope<S>>,
            "sequenced";<(TransactionDigest, ObjectID), SequenceNumber>,
            "schedule";<ObjectID, SequenceNumber>,
            "batches";<TxSequenceNumber, SignedBatch>,
            "last_consensus_index";<u64, ExecutionIndices>,
            "epochs";<EpochId, EpochInfoLocals>
        );
        Self {
            objects,
            owner_index,
            transactions,
            certificates,
            pending_execution,
            parent_sync,
            effects,
            sequenced,
            schedule,
            executed_sequence,
            batches,
            last_consensus_index,
            epochs,
        }
    }
}

fn dump<Q>(store: SuiDataStoreReadonly<Q>, table_name: &str) -> BTreeMap<String, String>
where
    Q: Eq + Serialize + for<'de> Deserialize<'de> + Debug,
{
    match table_name {
        OBJECTS_TABLE_NAME => {
            store.objects.try_catch_up_with_primary().unwrap();
            store
                .objects
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }
        OWNER_INDEX_TABLE_NAME => {
            store.owner_index.try_catch_up_with_primary().unwrap();

            store
                .owner_index
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }
        TX_TABLE_NAME => {
            store.transactions.try_catch_up_with_primary().unwrap();
            store
                .transactions
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        CERTS_TABLE_NAME => {
            store.certificates.try_catch_up_with_primary().unwrap();
            store
                .certificates
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        PENDING_EXECUTION => {
            store.pending_execution.try_catch_up_with_primary().unwrap();
            store
                .pending_execution
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        PARENT_SYNC_TABLE_NAME => {
            store.parent_sync.try_catch_up_with_primary().unwrap();
            store
                .parent_sync
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        EFFECTS_TABLE_NAME => {
            store.effects.try_catch_up_with_primary().unwrap();
            store
                .effects
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        SEQUENCED_TABLE_NAME => {
            store.sequenced.try_catch_up_with_primary().unwrap();
            store
                .sequenced
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        SCHEDULE_TABLE_NAME => {
            store.schedule.try_catch_up_with_primary().unwrap();
            store
                .schedule
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        EXEC_SEQ_TABLE_NAME => {
            store.executed_sequence.try_catch_up_with_primary().unwrap();
            store
                .executed_sequence
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        BATCHES_TABLE_NAME => {
            store.batches.try_catch_up_with_primary().unwrap();
            store
                .batches
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        LAST_CONSENSUS_TABLE_NAME => {
            store
                .last_consensus_index
                .try_catch_up_with_primary()
                .unwrap();
            store
                .last_consensus_index
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }

        EPOCH_TABLE_NAME => {
            store.epochs.try_catch_up_with_primary().unwrap();
            store
                .epochs
                .iter()
                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                .collect::<BTreeMap<_, _>>()
        }
        _ => panic!("No such table name"),
    }
}

pub fn dump_table(gateway: bool, path: PathBuf, table_name: &str) -> BTreeMap<String, String> {
    let temp_dir = tempfile::tempdir().unwrap().into_path();

    if gateway {
        dump(GatewayStoreReadOnly::open(path, temp_dir, None), table_name)
    } else {
        dump(
            AuthorityStoreReadOnly::open(path, temp_dir, None),
            table_name,
        )
    }
}
