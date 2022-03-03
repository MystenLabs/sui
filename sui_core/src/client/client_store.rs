// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use move_core_types::language_storage::StructTag;
use move_core_types::value::MoveStructLayout;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::path::PathBuf;
use std::sync::Arc;
use sui_types::object::Object;
use typed_store::rocks::DBMap;

const CERT_CF_NAME: &str = "certificates";
const SEQ_NUMBER_CF_NAME: &str = "object_sequence_numbers";
const OBJ_REF_CF_NAME: &str = "object_refs";
const TX_DIGEST_TO_CERT_CF_NAME: &str = "object_certs";
const PENDING_TRANSACTIONS_CF_NAME: &str = "pending_transactions";
const OBJECT_CF_NAME: &str = "objects";
const OBJECT_LAYOUTS_CF_NAME: &str = "object_layouts";

const MANAGED_ADDRESS_PATHS_CF_NAME: &str = "managed_address_paths";
const MANAGED_ADDRESS_SUBDIR: &str = "managed_addresses";

/// The address manager allows multiple addresses to be managed by one client
/// It also manages the different DB locations and in future, the configurations
/// Dir Structure
/// AddressManagerStore
///     |
///     ------ SingleAddressStore1
///     |
///     ------ SingleAddressStore2
///     |
///     ------ SingleAddressStore3
///     |
///     ------ SingleAddressStore4
pub struct ClientAddressManagerStore {
    // Address manager store path
    pub path: PathBuf,
    // Table of managed addresses to their paths
    // This is in the subdirectory MANAGED_ADDRESS_SUBDIR
    pub managed_address_paths: DBMap<SuiAddress, PathBuf>,
}
impl ClientAddressManagerStore {
    /// Open a store for the manager
    pub fn open(path: PathBuf) -> Self {
        // Open column family
        let db = client_store::init_store(path.clone(), vec![MANAGED_ADDRESS_PATHS_CF_NAME]);
        ClientAddressManagerStore {
            path,
            managed_address_paths: client_store::reopen_db(&db, MANAGED_ADDRESS_PATHS_CF_NAME),
        }
    }

    /// Make a DB path for a given address
    fn make_db_path_for_address(&self, address: SuiAddress) -> PathBuf {
        let mut path = self.path.clone();
        path.push(PathBuf::from(format!(
            "{}/{:02X}",
            MANAGED_ADDRESS_SUBDIR, address
        )));
        path
    }

    /// Add an address to be managed
    pub fn manage_new_address(
        &self,
        address: SuiAddress,
    ) -> Result<client_store::ClientSingleAddressStore, typed_store::rocks::TypedStoreError> {
        // Create an a path for this address
        let path = self.make_db_path_for_address(address);
        self.managed_address_paths.insert(&address, &path)?;
        Ok(ClientSingleAddressStore::new(path))
    }

    /// Gets managed address if any
    pub fn get_managed_address(
        &self,
        address: SuiAddress,
    ) -> Result<Option<client_store::ClientSingleAddressStore>, typed_store::rocks::TypedStoreError>
    {
        self.managed_address_paths
            .get(&address)
            .map(|opt_path| opt_path.map(ClientSingleAddressStore::new))
    }

    /// Check if an address is managed
    pub fn is_managed_address(
        &self,
        address: SuiAddress,
    ) -> Result<bool, typed_store::rocks::TypedStoreError> {
        self.managed_address_paths.contains_key(&address)
    }
}

/// This is the store of one address
pub struct ClientSingleAddressStore {
    /// Table of objects to transactions pending on the objects
    pub pending_transactions: DBMap<ObjectID, Transaction>,
    // The remaining fields are used to minimize networking, and may not always be persisted locally.
    /// Known certificates, indexed by TX digest.
    pub certificates: DBMap<TransactionDigest, CertifiedTransaction>,
    /// Confirmed objects with it's ref owned by the client.
    pub object_refs: DBMap<ObjectID, ObjectRef>,
    /// Certificate <-> object id linking map.
    pub object_certs: DBMap<ObjectRef, TransactionDigest>,
    /// Map from object ref to actual object to track object history
    /// There can be duplicates and we never delete objects
    pub objects: DBMap<ObjectRef, Object>,
    /// Map from Move object type to the layout of the object's Move contents.
    /// Should contain the layouts for all object types in `objects`, and
    /// possibly more.
    pub object_layouts: DBMap<StructTag, MoveStructLayout>,
}

impl ClientSingleAddressStore {
    pub fn new(path: PathBuf) -> Self {
        // Open column families
        let db = client_store::init_store(
            path,
            vec![
                PENDING_TRANSACTIONS_CF_NAME,
                CERT_CF_NAME,
                SEQ_NUMBER_CF_NAME,
                OBJ_REF_CF_NAME,
                TX_DIGEST_TO_CERT_CF_NAME,
                OBJECT_CF_NAME,
                OBJECT_LAYOUTS_CF_NAME,
            ],
        );

        ClientSingleAddressStore {
            pending_transactions: client_store::reopen_db(&db, PENDING_TRANSACTIONS_CF_NAME),
            certificates: client_store::reopen_db(&db, CERT_CF_NAME),
            object_refs: client_store::reopen_db(&db, OBJ_REF_CF_NAME),
            object_certs: client_store::reopen_db(&db, TX_DIGEST_TO_CERT_CF_NAME),
            objects: client_store::reopen_db(&db, OBJECT_CF_NAME),
            object_layouts: client_store::reopen_db(&db, OBJECT_LAYOUTS_CF_NAME),
        }
    }
}

/// General helper function for init a store
pub fn init_store(path: PathBuf, names: Vec<&str>) -> Arc<DBWithThreadMode<MultiThreaded>> {
    open_cf(&path, None, &names).expect("Cannot open DB.")
}
/// General helper function for (re-)opening a store
fn reopen_db<K, V>(db: &Arc<DBWithThreadMode<MultiThreaded>>, name: &str) -> DBMap<K, V> {
    DBMap::reopen(db, Some(name)).expect(&format!("Cannot open {} CF.", name)[..])
}
