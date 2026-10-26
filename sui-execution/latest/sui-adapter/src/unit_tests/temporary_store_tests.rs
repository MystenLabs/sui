// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::temporary_store::TemporaryStore;
use std::cell::Cell;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    SUI_PACKAGE_CONFIG_OBJECT_ID,
    base_types::{
        ConsensusObjectVersion, ObjectID, ObjectRef, SequenceNumber, SystemObjectVersions,
        TransactionDigest,
    },
    committee::EpochId,
    error::SuiResult,
    object::{MoveObject, Object, Owner},
    storage::{BackingPackageStore, ObjectStore, PackageObject, ParentSync, RuntimeObjectResolver},
    transaction::{InputObjectKind, InputObjects, ObjectReadResult, SharedObjectMutability},
};

struct CountingStore {
    package_config: Option<Object>,
    implicit_loads: Cell<usize>,
}

impl BackingPackageStore for CountingStore {
    fn get_package_object(&self, _package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        Ok(None)
    }
}

impl RuntimeObjectResolver for CountingStore {
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        _child: &ObjectID,
        _child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(None)
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        Ok(None)
    }
}

impl ObjectStore for CountingStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.package_config
            .as_ref()
            .filter(|object| object.id() == *object_id)
            .cloned()
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.get_object(object_id)
            .filter(|object| object.version() == version)
    }

    fn load_implicitly_read_system_object(
        &self,
        object_id: &ObjectID,
        version: ConsensusObjectVersion,
    ) -> Option<Object> {
        self.implicit_loads.set(self.implicit_loads.get() + 1);
        self.get_object_by_key(object_id, version.version)
    }
}

impl ParentSync for CountingStore {
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        None
    }
}

fn package_config(version: SequenceNumber) -> Object {
    Object::new_move(
        MoveObject::new_gas_coin(version, SUI_PACKAGE_CONFIG_OBJECT_ID, 1),
        Owner::Shared {
            initial_shared_version: SequenceNumber::MIN,
        },
        TransactionDigest::ZERO,
    )
}

fn system_object_versions(version: Option<SequenceNumber>) -> SystemObjectVersions {
    SystemObjectVersions::new(
        None,
        None,
        version.map(|version| ConsensusObjectVersion {
            initial_shared_version: SequenceNumber::MIN,
            version,
        }),
    )
}

fn temporary_store<'a>(
    store: &'a CountingStore,
    protocol_config: &'a ProtocolConfig,
    explicit_input: Option<Object>,
    assigned_version: Option<SequenceNumber>,
) -> TemporaryStore<'a> {
    let input_objects = InputObjects::new(
        explicit_input
            .into_iter()
            .map(|object| {
                ObjectReadResult::new(
                    InputObjectKind::SharedMoveObject {
                        id: SUI_PACKAGE_CONFIG_OBJECT_ID,
                        initial_shared_version: SequenceNumber::MIN,
                        mutability: SharedObjectMutability::Mutable,
                    },
                    object.into(),
                )
            })
            .collect(),
    );
    TemporaryStore::new_for_testing(
        store,
        input_objects,
        protocol_config,
        system_object_versions(assigned_version),
    )
}

#[test]
fn explicit_package_config_input_is_not_recorded_as_an_implicit_read() {
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    let version = SequenceNumber::from_u64(7);
    let store = CountingStore {
        package_config: None,
        implicit_loads: Cell::new(0),
    };
    let temporary_store = temporary_store(
        &store,
        &protocol_config,
        Some(package_config(version)),
        Some(version),
    );

    assert_eq!(
        temporary_store
            .package_config_root_version_for_testing()
            .unwrap(),
        Some(version)
    );
    assert_eq!(store.implicit_loads.get(), 0);
    assert_eq!(
        temporary_store.loaded_system_object_version_for_testing(SUI_PACKAGE_CONFIG_OBJECT_ID),
        None
    );
}

#[test]
fn implicit_package_config_input_is_loaded_once() {
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    let version = SequenceNumber::from_u64(7);
    let store = CountingStore {
        package_config: Some(package_config(version)),
        implicit_loads: Cell::new(0),
    };
    let temporary_store = temporary_store(&store, &protocol_config, None, Some(version));

    assert_eq!(
        temporary_store
            .package_config_root_version_for_testing()
            .unwrap(),
        Some(version)
    );
    assert_eq!(
        temporary_store
            .package_config_root_version_for_testing()
            .unwrap(),
        Some(version)
    );
    assert_eq!(store.implicit_loads.get(), 1);
    assert_eq!(
        temporary_store.loaded_system_object_version_for_testing(SUI_PACKAGE_CONFIG_OBJECT_ID),
        Some(version)
    );
}

#[test]
fn absent_package_config_assignment_disables_policy_lookup() {
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    let version = SequenceNumber::from_u64(7);
    let store = CountingStore {
        package_config: Some(package_config(version)),
        implicit_loads: Cell::new(0),
    };
    let temporary_store = temporary_store(&store, &protocol_config, None, None);

    assert_eq!(
        temporary_store
            .package_config_root_version_for_testing()
            .unwrap(),
        None
    );
    assert_eq!(store.implicit_loads.get(), 0);
}

#[test]
fn missing_assigned_package_config_version_is_an_error() {
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    let version = SequenceNumber::from_u64(7);
    let store = CountingStore {
        package_config: None,
        implicit_loads: Cell::new(0),
    };
    let temporary_store = temporary_store(&store, &protocol_config, None, Some(version));

    assert!(
        temporary_store
            .package_config_root_version_for_testing()
            .is_err()
    );
    assert_eq!(store.implicit_loads.get(), 1);
    assert_eq!(
        temporary_store.loaded_system_object_version_for_testing(SUI_PACKAGE_CONFIG_OBJECT_ID),
        None
    );
}
