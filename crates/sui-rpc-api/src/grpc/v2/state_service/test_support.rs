// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared rpc-store + object-store fixture for `StateService` handler unit tests.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use move_binary_format::file_format::Ability;
use move_binary_format::file_format::AbilitySet;
use move_binary_format::file_format::CompiledModule;
use move_binary_format::file_format::DatatypeHandle;
use move_binary_format::file_format::DatatypeHandleIndex;
use move_binary_format::file_format::DatatypeTyParameter;
use move_binary_format::file_format::FieldDefinition;
use move_binary_format::file_format::IdentifierIndex;
use move_binary_format::file_format::ModuleHandle;
use move_binary_format::file_format::ModuleHandleIndex;
use move_binary_format::file_format::SignatureToken;
use move_binary_format::file_format::StructDefinition;
use move_binary_format::file_format::StructFieldInformation;
use move_binary_format::file_format::empty_module;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_consistent_store::Watermark;
use sui_rpc_store::RpcStoreReader;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::schema::balance as balance_schema;
use sui_rpc_store::schema::object_by_owner as owner_schema;
use sui_rpc_store::schema::object_by_type as type_schema;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::EpochId;
use sui_types::base_types::MoveObjectType;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::committee::Committee;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::DynamicFieldInfo;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::error::SuiResult;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VersionedFullCheckpointContents;
use sui_types::move_package::MovePackage;
use sui_types::object::Data;
use sui_types::object::MoveObject;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::CheckpointIndexes;
use sui_types::storage::ObjectKey;
use sui_types::storage::ObjectStore;
use sui_types::storage::PackageObject;
use sui_types::storage::ReadStore;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;
use sui_types::storage::RuntimeObjectResolver;
use sui_types::storage::error::Result as StorageResult;
use sui_types::transaction::VerifiedTransaction;

use crate::RpcService;

/// Real `sui-rpc-store` (with configurable RocksDB snapshot window) paired with an in-memory
/// versioned object store and a minimal `0x2` Move package for `FieldVisitor` layout decoding.
pub(super) struct TestStateStore {
    db: Db,
    store: RpcStoreReader,
    objects: RwLock<BTreeMap<(ObjectID, SequenceNumber), Object>>,
    packages: BTreeMap<ObjectID, Object>,
    lowest_available_objects: AtomicU64,
}

impl TestStateStore {
    pub(super) fn open(snapshot_capacity: usize) -> (tempfile::TempDir, Arc<Self>, RpcService) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(
            dir.path(),
            DbOptions {
                snapshot_capacity,
                ..DbOptions::default()
            },
        )
        .unwrap();
        let store = RpcStoreReader::new(db.clone(), Arc::new(schema));
        let mut packages = BTreeMap::new();
        packages.insert(SUI_FRAMEWORK_PACKAGE_ID, synthetic_framework_package());
        let state = Arc::new(Self {
            db,
            store,
            objects: RwLock::new(BTreeMap::new()),
            packages,
            lowest_available_objects: AtomicU64::new(0),
        });
        let service = RpcService::new(state.clone());
        (dir, state, service)
    }

    pub(super) fn put_object(&self, object: Object) {
        self.apply_object_change(None, object);
    }

    pub(super) fn replace_object(&self, old: &Object, new: Object) {
        self.apply_object_change(Some(old), new);
    }

    fn apply_object_change(&self, old: Option<&Object>, new: Object) {
        let mut batch = self.db.batch();
        if let Some(old) = old {
            if let Some((key, _)) = owner_schema::store(old) {
                batch
                    .delete(&self.store.schema().object_by_owner, &key)
                    .unwrap();
            }
            if let Some((key, _)) = type_schema::store(old) {
                batch
                    .delete(&self.store.schema().object_by_type, &key)
                    .unwrap();
            }
        }
        if let Some((key, value)) = owner_schema::store(&new) {
            batch
                .put(&self.store.schema().object_by_owner, &key, &value)
                .unwrap();
        }
        if let Some((key, value)) = type_schema::store(&new) {
            batch
                .put(&self.store.schema().object_by_type, &key, &value)
                .unwrap();
        }
        self.objects
            .write()
            .unwrap()
            .insert((new.id(), new.version()), new);
        batch.commit().unwrap();
    }

    /// Delete one stored object version without touching the rpc-store indexes (simulates a
    /// missing version / pruning race or post-index deletion).
    pub(super) fn drop_object_version(&self, id: ObjectID, version: SequenceNumber) {
        self.objects.write().unwrap().remove(&(id, version));
    }

    pub(super) fn add_balance(
        &self,
        owner: SuiAddress,
        coin_type: StructTag,
        coin: i128,
        address: i128,
    ) {
        let (key, value) =
            balance_schema::delta(owner, TypeTag::Struct(Box::new(coin_type)), coin, address);
        let mut batch = self.db.batch();
        batch
            .merge(&self.store.schema().balance, &key, &value)
            .unwrap();
        batch.commit().unwrap();
    }

    pub(super) fn take_snapshot(&self, checkpoint: u64) {
        self.db.take_snapshot(Watermark::for_checkpoint(checkpoint));
    }

    pub(super) fn set_lowest_available_objects(&self, checkpoint: u64) {
        self.lowest_available_objects
            .store(checkpoint, Ordering::SeqCst);
    }
}

pub(super) fn make_struct_object(
    id: ObjectID,
    version: u64,
    owner: Owner,
    struct_tag: StructTag,
    contents: Vec<u8>,
) -> Object {
    assert!(contents.starts_with(id.as_ref()));
    let move_obj = unsafe {
        MoveObject::new_from_execution_with_limit(
            MoveObjectType::from(struct_tag),
            true,
            SequenceNumber::from_u64(version),
            contents,
            1024 * 1024,
        )
        .unwrap()
    };
    Object::new_move(move_obj, owner, TransactionDigest::genesis_marker())
}

pub(super) fn make_coin_object(
    id: ObjectID,
    version: u64,
    owner: Owner,
    coin_type: StructTag,
    balance: u64,
) -> Object {
    let move_obj = MoveObject::new_coin(
        TypeTag::Struct(Box::new(coin_type)),
        SequenceNumber::from_u64(version),
        id,
        balance,
    );
    Object::new_move(move_obj, owner, TransactionDigest::genesis_marker())
}

/// Build a `0x2::dynamic_field::Field<u64, u64>` object owned by `parent`.
pub(super) fn make_dynamic_field_u64(
    parent: ObjectID,
    field_id: ObjectID,
    version: u64,
    name: u64,
    value: u64,
) -> Object {
    let field_type = DynamicFieldInfo::dynamic_field_type(TypeTag::U64, TypeTag::U64);
    let mut bcs = Vec::with_capacity(ObjectID::LENGTH + 16);
    bcs.extend_from_slice(field_id.as_ref());
    bcs.extend_from_slice(&name.to_le_bytes());
    bcs.extend_from_slice(&value.to_le_bytes());
    make_struct_object(
        field_id,
        version,
        Owner::ObjectOwner(parent.into()),
        field_type,
        bcs,
    )
}

/// Build a `0x2::dynamic_field::Field<0x2::dynamic_object_field::Wrapper<u64>, 0x2::object::ID>`
/// object owned by `parent` pointing at `child_id`.
pub(super) fn make_dynamic_object_field_u64(
    parent: ObjectID,
    field_id: ObjectID,
    version: u64,
    name: u64,
    child_id: ObjectID,
) -> Object {
    let wrapper = DynamicFieldInfo::dynamic_object_field_wrapper(TypeTag::U64);
    let id_tag = sui_types::parse_sui_struct_tag("0x2::object::ID").unwrap();
    let field_type = DynamicFieldInfo::dynamic_field_type(
        TypeTag::Struct(Box::new(wrapper)),
        TypeTag::Struct(Box::new(id_tag)),
    );
    let mut bcs = Vec::with_capacity(ObjectID::LENGTH * 2 + 8);
    bcs.extend_from_slice(field_id.as_ref());
    bcs.extend_from_slice(&name.to_le_bytes());
    bcs.extend_from_slice(child_id.as_ref());
    make_struct_object(
        field_id,
        version,
        Owner::ObjectOwner(parent.into()),
        field_type,
        bcs,
    )
}

/// Minimal synthetic `0x2` Move package defining `object::ID`, `object::UID`,
/// `dynamic_field::Field<Name, Value>`, and `dynamic_object_field::Wrapper<Name>` so
/// `FieldVisitor` and `RenderContext::type_layout` decode test dynamic fields cleanly.
fn synthetic_framework_package() -> Object {
    let object_module = {
        let mut m: CompiledModule = empty_module();
        m.address_identifiers = vec![SUI_FRAMEWORK_ADDRESS];
        m.identifiers = vec![
            Identifier::new("object").unwrap(),
            Identifier::new("ID").unwrap(),
            Identifier::new("bytes").unwrap(),
            Identifier::new("UID").unwrap(),
            Identifier::new("id").unwrap(),
        ];
        m.module_handles = vec![ModuleHandle {
            address: move_binary_format::file_format::AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }];
        m.datatype_handles = vec![
            DatatypeHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(1),
                abilities: AbilitySet::ALL,
                type_parameters: vec![],
            },
            DatatypeHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(3),
                abilities: AbilitySet::EMPTY | Ability::Store,
                type_parameters: vec![],
            },
        ];
        m.struct_defs = vec![
            StructDefinition {
                struct_handle: DatatypeHandleIndex(0),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(2),
                    signature: move_binary_format::file_format::TypeSignature(
                        SignatureToken::Address,
                    ),
                }]),
            },
            StructDefinition {
                struct_handle: DatatypeHandleIndex(1),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(4),
                    signature: move_binary_format::file_format::TypeSignature(
                        SignatureToken::Datatype(DatatypeHandleIndex(0)),
                    ),
                }]),
            },
        ];
        m
    };

    let dynamic_field_module = {
        let mut m: CompiledModule = empty_module();
        m.address_identifiers = vec![SUI_FRAMEWORK_ADDRESS];
        m.identifiers = vec![
            Identifier::new("dynamic_field").unwrap(),
            Identifier::new("object").unwrap(),
            Identifier::new("UID").unwrap(),
            Identifier::new("Field").unwrap(),
            Identifier::new("id").unwrap(),
            Identifier::new("name").unwrap(),
            Identifier::new("value").unwrap(),
        ];
        m.module_handles = vec![
            ModuleHandle {
                address: move_binary_format::file_format::AddressIdentifierIndex(0),
                name: IdentifierIndex(0),
            },
            ModuleHandle {
                address: move_binary_format::file_format::AddressIdentifierIndex(0),
                name: IdentifierIndex(1),
            },
        ];
        m.datatype_handles = vec![
            DatatypeHandle {
                module: ModuleHandleIndex(1),
                name: IdentifierIndex(2),
                abilities: AbilitySet::EMPTY | Ability::Store,
                type_parameters: vec![],
            },
            DatatypeHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(3),
                abilities: AbilitySet::EMPTY | Ability::Key,
                type_parameters: vec![
                    DatatypeTyParameter {
                        constraints: AbilitySet::EMPTY
                            | Ability::Copy
                            | Ability::Drop
                            | Ability::Store,
                        is_phantom: false,
                    },
                    DatatypeTyParameter {
                        constraints: AbilitySet::EMPTY | Ability::Store,
                        is_phantom: false,
                    },
                ],
            },
        ];
        m.struct_defs = vec![StructDefinition {
            struct_handle: DatatypeHandleIndex(1),
            field_information: StructFieldInformation::Declared(vec![
                FieldDefinition {
                    name: IdentifierIndex(4),
                    signature: move_binary_format::file_format::TypeSignature(
                        SignatureToken::Datatype(DatatypeHandleIndex(0)),
                    ),
                },
                FieldDefinition {
                    name: IdentifierIndex(5),
                    signature: move_binary_format::file_format::TypeSignature(
                        SignatureToken::TypeParameter(0),
                    ),
                },
                FieldDefinition {
                    name: IdentifierIndex(6),
                    signature: move_binary_format::file_format::TypeSignature(
                        SignatureToken::TypeParameter(1),
                    ),
                },
            ]),
        }];
        m
    };

    let dynamic_object_field_module = {
        let mut m: CompiledModule = empty_module();
        m.address_identifiers = vec![SUI_FRAMEWORK_ADDRESS];
        m.identifiers = vec![
            Identifier::new("dynamic_object_field").unwrap(),
            Identifier::new("Wrapper").unwrap(),
            Identifier::new("name").unwrap(),
        ];
        m.module_handles = vec![ModuleHandle {
            address: move_binary_format::file_format::AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }];
        m.datatype_handles = vec![DatatypeHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(1),
            abilities: AbilitySet::EMPTY | Ability::Copy | Ability::Drop | Ability::Store,
            type_parameters: vec![DatatypeTyParameter {
                constraints: AbilitySet::EMPTY | Ability::Copy | Ability::Drop | Ability::Store,
                is_phantom: false,
            }],
        }];
        m.struct_defs = vec![StructDefinition {
            struct_handle: DatatypeHandleIndex(0),
            field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                name: IdentifierIndex(2),
                signature: move_binary_format::file_format::TypeSignature(
                    SignatureToken::TypeParameter(0),
                ),
            }]),
        }];
        m
    };

    let package = MovePackage::new_system(
        SequenceNumber::from_u64(1),
        &[
            object_module,
            dynamic_field_module,
            dynamic_object_field_module,
        ],
        [],
    );
    Object::new_package_from_data(Data::Package(package), TransactionDigest::genesis_marker())
}

impl ObjectStore for TestStateStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.objects
            .read()
            .unwrap()
            .range((*object_id, SequenceNumber::MIN)..=(*object_id, SequenceNumber::MAX))
            .next_back()
            .map(|(_, obj)| obj.clone())
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.objects
            .read()
            .unwrap()
            .get(&(*object_id, version))
            .cloned()
    }
}

impl BackingPackageStore for TestStateStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        Ok(self
            .packages
            .get(package_id)
            .cloned()
            .map(PackageObject::new))
    }
}

impl RuntimeObjectResolver for TestStateStore {
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self
            .objects
            .read()
            .unwrap()
            .range((*child, SequenceNumber::MIN)..=(*child, child_version_upper_bound))
            .next_back()
            .map(|(_, obj)| obj.clone()))
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        Ok(self.get_object_by_key(receiving_object_id, receive_object_at_version))
    }
}

impl ReadStore for TestStateStore {
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.store.get_committee(epoch)
    }

    fn get_latest_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        self.store.get_latest_checkpoint()
    }

    fn get_highest_verified_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        self.store.get_highest_verified_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        self.store.get_highest_synced_checkpoint()
    }

    fn get_lowest_available_checkpoint(&self) -> StorageResult<CheckpointSequenceNumber> {
        self.store.get_lowest_available_checkpoint()
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.store.get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.store
            .get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.store.get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        self.store
            .get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.store.get_transaction(tx_digest)
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.store.get_transaction_effects(tx_digest)
    }

    fn get_events(&self, event_digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.store.get_events(event_digest)
    }

    fn get_full_checkpoint_contents(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
        digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        self.store
            .get_full_checkpoint_contents(sequence_number, digest)
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        self.store.get_unchanged_loaded_runtime_objects(digest)
    }

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.store.get_transaction_checkpoint(digest)
    }
}

impl RpcStateReader for TestStateStore {
    fn get_lowest_available_checkpoint_objects(&self) -> StorageResult<CheckpointSequenceNumber> {
        Ok(self.lowest_available_objects.load(Ordering::SeqCst))
    }

    fn get_chain_identifier(&self) -> StorageResult<ChainIdentifier> {
        Ok(ChainIdentifier::from(CheckpointDigest::default()))
    }

    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        self.store.indexes()
    }

    fn indexes_at_checkpoint(
        &self,
        checkpoint: Option<CheckpointSequenceNumber>,
    ) -> CheckpointIndexes {
        self.store.indexes_at_checkpoint(checkpoint)
    }

    fn retained_checkpoint_indexes_range(
        &self,
    ) -> Option<RangeInclusive<CheckpointSequenceNumber>> {
        self.store.retained_checkpoint_indexes_range()
    }

    fn layout_package_store(&self) -> Box<dyn BackingPackageStore + Send + Sync + '_> {
        Box::new(self)
    }
}

pub(super) fn struct_tag(s: &str) -> StructTag {
    sui_types::parse_sui_struct_tag(s).unwrap()
}

pub(super) fn test_address(byte: u8) -> SuiAddress {
    SuiAddress::from(ObjectID::from_single_byte(byte))
}

pub(super) fn test_object_id(byte: u8) -> ObjectID {
    ObjectID::from_single_byte(byte)
}

pub(super) fn dummy_struct_bytes(id: ObjectID, payload: u64) -> Vec<u8> {
    let mut bcs = Vec::with_capacity(ObjectID::LENGTH + 8);
    bcs.extend_from_slice(id.as_ref());
    bcs.extend_from_slice(&payload.to_le_bytes());
    bcs
}
