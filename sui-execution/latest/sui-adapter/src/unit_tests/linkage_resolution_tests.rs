// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::{PackageMetadata, PackageStore},
    static_programmable_transactions::linkage::{
        config::{LinkageConfig, ResolutionConfig},
        resolution::{ConstraintKind, LinkageStoreResolver, ResolutionTable, VersionConstraint},
    },
};
use move_binary_format::binary_config::BinaryConfig;
use move_core_types::identifier::IdentStr;
use move_vm_runtime::shared::types::{OriginalId, VersionId};
use std::collections::{BTreeMap, BTreeSet};
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    base_types::ObjectID,
    error::{ExecutionError, SuiResult},
    execution_status::ExecutionErrorKind,
    id::ID,
    package_config::MinVersion,
};

#[derive(Clone)]
struct TestPackage {
    id: ObjectID,
    original_id: ObjectID,
    linkage: BTreeMap<OriginalId, VersionId>,
}

impl PackageMetadata for TestPackage {
    fn version(&self) -> u64 {
        u64::from(self.id.into_bytes()[ObjectID::LENGTH - 1])
    }

    fn version_id(&self) -> ObjectID {
        self.id
    }

    fn original_id(&self) -> ObjectID {
        self.original_id
    }

    fn linkage_table(&self) -> BTreeMap<OriginalId, VersionId> {
        self.linkage.clone()
    }
}

struct TestStore(BTreeMap<ObjectID, TestPackage>);

impl PackageStore for TestStore {
    type Package = TestPackage;

    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Self::Package>> {
        Ok(self.0.get(id).cloned())
    }

    fn resolve_type_to_defining_id(
        &self,
        _module_address: ObjectID,
        _module_name: &IdentStr,
        _type_name: &IdentStr,
    ) -> SuiResult<Option<ObjectID>> {
        Ok(None)
    }
}

#[test]
fn minversion_resolution_records_referenced_and_selected_packages() {
    let original_id = ObjectID::from_single_byte(1);
    let historical_id = ObjectID::from_single_byte(10);
    let selected_id = ObjectID::from_single_byte(12);
    let store = TestStore(BTreeMap::from([
        (
            historical_id,
            TestPackage {
                id: historical_id,
                original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            selected_id,
            TestPackage {
                id: selected_id,
                original_id,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let mut resolution_table = ResolutionTable::empty(config);
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok((id == original_id).then_some(MinVersion {
            version: 12,
            package_id: ID::new(selected_id),
        }))
    };
    let mut resolver =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

    resolver
        .resolve_package(
            &mut resolution_table,
            historical_id,
            ConstraintKind::AtLeast,
            ConstraintKind::AtLeast,
        )
        .unwrap();

    assert!(matches!(
        resolution_table.resolution_table.get(&original_id),
        Some(VersionConstraint::AtLeast(12, id)) if *id == selected_id
    ));
    for (package_id, version) in [(historical_id, 10), (selected_id, 12)] {
        let resolution = resolution_table
            .all_versions_resolution_table
            .get(&package_id)
            .unwrap();
        assert_eq!(resolution.original_id, original_id);
        assert_eq!(resolution.version, Some(version));
    }
}

#[test]
fn selected_package_collection_uses_selected_package_dependencies() {
    let original_id = ObjectID::from_single_byte(1);
    let historical_id = ObjectID::from_single_byte(10);
    let selected_id = ObjectID::from_single_byte(12);
    let selected_dependency = ObjectID::from_single_byte(100);
    let store = TestStore(BTreeMap::from([
        (
            historical_id,
            TestPackage {
                id: historical_id,
                original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            selected_id,
            TestPackage {
                id: selected_id,
                original_id,
                linkage: BTreeMap::from([(selected_dependency.into(), selected_dependency.into())]),
            },
        ),
        (
            selected_dependency,
            TestPackage {
                id: selected_dependency,
                original_id: selected_dependency,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let resolution_table = ResolutionTable::empty(config);
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok((id == original_id).then_some(MinVersion {
            version: 12,
            package_id: ID::new(selected_id),
        }))
    };
    let mut builder =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));
    let mut original_ids = BTreeSet::new();

    builder
        .collect_original_ids(&resolution_table, historical_id, &mut original_ids)
        .unwrap();

    assert_eq!(
        original_ids,
        BTreeSet::from([original_id, selected_dependency])
    );
}

#[test]
fn system_packages_do_not_read_package_config() {
    let store = TestStore(BTreeMap::new());
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let resolution_table = ResolutionTable::empty(config);
    let minversion_resolver = |_| -> Result<Option<MinVersion>, ExecutionError> {
        panic!("system packages must not read PackageConfig")
    };
    let mut resolver =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));
    let mut original_ids = BTreeSet::new();

    resolver
        .collect_original_ids(
            &resolution_table,
            SUI_FRAMEWORK_PACKAGE_ID,
            &mut original_ids,
        )
        .unwrap();

    assert!(original_ids.is_empty());
}

#[test]
fn minversion_does_not_downgrade_equal_or_newer_packages() {
    let original_id = ObjectID::from_single_byte(1);
    let older_id = ObjectID::from_single_byte(10);
    let stable_id = ObjectID::from_single_byte(12);
    let newer_id = ObjectID::from_single_byte(13);
    let store = TestStore(BTreeMap::from([
        (
            older_id,
            TestPackage {
                id: older_id,
                original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            stable_id,
            TestPackage {
                id: stable_id,
                original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            newer_id,
            TestPackage {
                id: newer_id,
                original_id,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok((id == original_id).then_some(MinVersion {
            version: 12,
            package_id: ID::new(stable_id),
        }))
    };
    let mut resolver =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

    assert_eq!(
        resolver.load_package(&older_id).unwrap().version_id(),
        stable_id
    );
    assert_eq!(
        resolver.load_package(&stable_id).unwrap().version_id(),
        stable_id
    );
    assert_eq!(
        resolver.load_package(&newer_id).unwrap().version_id(),
        newer_id
    );
}

#[test]
fn invalid_minversion_selections_are_rejected() {
    let original_id = ObjectID::from_single_byte(1);
    let historical_id = ObjectID::from_single_byte(10);
    let selected_id = ObjectID::from_single_byte(12);
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok((id == original_id).then_some(MinVersion {
            version: 12,
            package_id: ID::new(selected_id),
        }))
    };

    let historical = TestPackage {
        id: historical_id,
        original_id,
        linkage: BTreeMap::new(),
    };
    let cases = [
        // The selected package is absent.
        TestStore(BTreeMap::from([(historical_id, historical.clone())])),
        // The selected package belongs to another original package family.
        TestStore(BTreeMap::from([
            (historical_id, historical.clone()),
            (
                selected_id,
                TestPackage {
                    id: selected_id,
                    original_id: ObjectID::from_single_byte(2),
                    linkage: BTreeMap::new(),
                },
            ),
        ])),
        // The selected package ID does not have the configured version.
        TestStore(BTreeMap::from([
            (historical_id, historical),
            (
                selected_id,
                TestPackage {
                    id: ObjectID::from_single_byte(13),
                    original_id,
                    linkage: BTreeMap::new(),
                },
            ),
        ])),
    ];

    for store in &cases {
        let mut resolver =
            LinkageStoreResolver::<_, ExecutionError>::new(store, Some(&minversion_resolver));
        let Err(error) = resolver.load_package(&historical_id) else {
            panic!("invalid minversion selection unexpectedly resolved");
        };
        assert_eq!(error.kind(), &ExecutionErrorKind::InvariantViolation);
    }
}

#[test]
fn selected_cyclic_linkage_resolution_terminates() {
    let root_original_id = ObjectID::from_single_byte(1);
    let root_historical_id = ObjectID::from_single_byte(10);
    let root_selected_id = ObjectID::from_single_byte(12);
    let dependency_original_id = ObjectID::from_single_byte(2);
    let dependency_historical_id = ObjectID::from_single_byte(20);
    let dependency_selected_id = ObjectID::from_single_byte(22);
    let store = TestStore(BTreeMap::from([
        (
            root_historical_id,
            TestPackage {
                id: root_historical_id,
                original_id: root_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            root_selected_id,
            TestPackage {
                id: root_selected_id,
                original_id: root_original_id,
                linkage: BTreeMap::from([(
                    dependency_original_id.into(),
                    dependency_historical_id.into(),
                )]),
            },
        ),
        (
            dependency_historical_id,
            TestPackage {
                id: dependency_historical_id,
                original_id: dependency_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            dependency_selected_id,
            TestPackage {
                id: dependency_selected_id,
                original_id: dependency_original_id,
                linkage: BTreeMap::from([(root_original_id.into(), root_historical_id.into())]),
            },
        ),
    ]));
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok(match id {
            id if id == root_original_id => Some(MinVersion {
                version: 12,
                package_id: ID::new(root_selected_id),
            }),
            id if id == dependency_original_id => Some(MinVersion {
                version: 22,
                package_id: ID::new(dependency_selected_id),
            }),
            _ => None,
        })
    };
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let mut resolution_table = ResolutionTable::empty(config);
    let mut resolver =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

    resolver
        .resolve_package(
            &mut resolution_table,
            root_historical_id,
            ConstraintKind::AtLeast,
            ConstraintKind::AtLeast,
        )
        .unwrap();

    assert_eq!(resolution_table.resolution_table.len(), 2);
}

#[test]
fn selected_flattened_linkage_resolves_transitive_minversions() {
    let root_original_id = ObjectID::from_single_byte(1);
    let root_historical_id = ObjectID::from_single_byte(10);
    let root_selected_id = ObjectID::from_single_byte(12);
    let left = ObjectID::from_single_byte(2);
    let right = ObjectID::from_single_byte(3);
    let shared_original_id = ObjectID::from_single_byte(4);
    let shared_historical_id = ObjectID::from_single_byte(40);
    let shared_selected_id = ObjectID::from_single_byte(42);
    let tail = ObjectID::from_single_byte(5);
    let store = TestStore(BTreeMap::from([
        (
            root_historical_id,
            TestPackage {
                id: root_historical_id,
                original_id: root_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            root_selected_id,
            TestPackage {
                id: root_selected_id,
                original_id: root_original_id,
                linkage: BTreeMap::from([
                    (left.into(), left.into()),
                    (right.into(), right.into()),
                    (shared_original_id.into(), shared_historical_id.into()),
                    (tail.into(), tail.into()),
                ]),
            },
        ),
        (
            left,
            TestPackage {
                id: left,
                original_id: left,
                linkage: BTreeMap::from([(shared_original_id.into(), shared_historical_id.into())]),
            },
        ),
        (
            right,
            TestPackage {
                id: right,
                original_id: right,
                linkage: BTreeMap::from([(shared_original_id.into(), shared_historical_id.into())]),
            },
        ),
        (
            shared_historical_id,
            TestPackage {
                id: shared_historical_id,
                original_id: shared_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            shared_selected_id,
            TestPackage {
                id: shared_selected_id,
                original_id: shared_original_id,
                linkage: BTreeMap::from([(tail.into(), tail.into())]),
            },
        ),
        (
            tail,
            TestPackage {
                id: tail,
                original_id: tail,
                linkage: BTreeMap::from([(right.into(), right.into())]),
            },
        ),
    ]));
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok(match id {
            id if id == root_original_id => Some(MinVersion {
                version: 12,
                package_id: ID::new(root_selected_id),
            }),
            id if id == shared_original_id => Some(MinVersion {
                version: 42,
                package_id: ID::new(shared_selected_id),
            }),
            _ => None,
        })
    };
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let mut resolution_table = ResolutionTable::empty(config);
    let mut resolver =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

    resolver
        .resolve_package(
            &mut resolution_table,
            root_historical_id,
            ConstraintKind::AtLeast,
            ConstraintKind::AtLeast,
        )
        .unwrap();

    assert_eq!(resolution_table.resolution_table.len(), 5);
    assert!(matches!(
        resolution_table.resolution_table.get(&shared_original_id),
        Some(VersionConstraint::AtLeast(42, id)) if *id == shared_selected_id
    ));
    assert!(resolution_table.resolution_table.contains_key(&tail));
}

#[test]
fn exact_expansion_is_not_suppressed_by_at_least_expansion() {
    let root_original_id = ObjectID::from_single_byte(1);
    let historical_root_id = ObjectID::from_single_byte(10);
    let selected_root_id = ObjectID::from_single_byte(12);
    let dependency = ObjectID::from_single_byte(2);
    let store = TestStore(BTreeMap::from([
        (
            historical_root_id,
            TestPackage {
                id: historical_root_id,
                original_id: root_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            selected_root_id,
            TestPackage {
                id: selected_root_id,
                original_id: root_original_id,
                linkage: BTreeMap::from([(dependency.into(), dependency.into())]),
            },
        ),
        (
            dependency,
            TestPackage {
                id: dependency,
                original_id: dependency,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok((id == root_original_id).then_some(MinVersion {
            version: 12,
            package_id: ID::new(selected_root_id),
        }))
    };
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let mut resolution_table = ResolutionTable::empty(config);
    let mut builder =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

    // Both historical references select the same v12 package. The second, exact traversal
    // must still expand its dependency under Exact rather than being suppressed by the first.
    builder
        .resolve_package(
            &mut resolution_table,
            historical_root_id,
            ConstraintKind::AtLeast,
            ConstraintKind::AtLeast,
        )
        .unwrap();
    builder
        .resolve_package(
            &mut resolution_table,
            historical_root_id,
            ConstraintKind::Exact,
            ConstraintKind::Exact,
        )
        .unwrap();

    assert!(matches!(
        resolution_table.resolution_table.get(&root_original_id),
        Some(VersionConstraint::Exact(12, id)) if *id == selected_root_id
    ));
    assert!(matches!(
        resolution_table.resolution_table.get(&dependency),
        Some(VersionConstraint::Exact(2, id)) if *id == dependency
    ));
}

#[test]
fn exact_dependency_expansion_is_not_suppressed_by_at_least_dependencies() {
    let root_original_id = ObjectID::from_single_byte(1);
    let root_historical_id = ObjectID::from_single_byte(10);
    let root_selected_id = ObjectID::from_single_byte(12);
    let dependency = ObjectID::from_single_byte(2);
    let store = TestStore(BTreeMap::from([
        (
            root_historical_id,
            TestPackage {
                id: root_historical_id,
                original_id: root_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            root_selected_id,
            TestPackage {
                id: root_selected_id,
                original_id: root_original_id,
                linkage: BTreeMap::from([(dependency.into(), dependency.into())]),
            },
        ),
        (
            dependency,
            TestPackage {
                id: dependency,
                original_id: dependency,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok((id == root_original_id).then_some(MinVersion {
            version: 12,
            package_id: ID::new(root_selected_id),
        }))
    };
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let mut resolution_table = ResolutionTable::empty(config);
    let mut builder =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

    builder
        .resolve_package(
            &mut resolution_table,
            root_historical_id,
            ConstraintKind::Exact,
            ConstraintKind::AtLeast,
        )
        .unwrap();
    builder
        .resolve_package(
            &mut resolution_table,
            root_historical_id,
            ConstraintKind::Exact,
            ConstraintKind::Exact,
        )
        .unwrap();

    assert!(matches!(
        resolution_table.resolution_table.get(&dependency),
        Some(VersionConstraint::Exact(2, id)) if *id == dependency
    ));
}

#[test]
fn unchanged_dependencies_use_the_root_flattened_linkage() {
    let root_original_id = ObjectID::from_single_byte(1);
    let root_id = ObjectID::from_single_byte(10);
    let dependency_original_id = ObjectID::from_single_byte(2);
    let dependency_id = ObjectID::from_single_byte(20);
    let tail_original_id = ObjectID::from_single_byte(3);
    let tail_old_id = ObjectID::from_single_byte(30);
    let tail_new_id = ObjectID::from_single_byte(31);
    let store = TestStore(BTreeMap::from([
        (
            root_id,
            TestPackage {
                id: root_id,
                original_id: root_original_id,
                linkage: BTreeMap::from([
                    (dependency_original_id.into(), dependency_id.into()),
                    (tail_original_id.into(), tail_new_id.into()),
                ]),
            },
        ),
        (
            dependency_id,
            TestPackage {
                id: dependency_id,
                original_id: dependency_original_id,
                linkage: BTreeMap::from([(tail_original_id.into(), tail_old_id.into())]),
            },
        ),
        (
            tail_old_id,
            TestPackage {
                id: tail_old_id,
                original_id: tail_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            tail_new_id,
            TestPackage {
                id: tail_new_id,
                original_id: tail_original_id,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let mut resolution_table = ResolutionTable::empty(config);
    let mut resolver = LinkageStoreResolver::<_, ExecutionError>::new(&store, None);

    resolver
        .resolve_package(
            &mut resolution_table,
            root_id,
            ConstraintKind::Exact,
            ConstraintKind::Exact,
        )
        .unwrap();

    assert!(matches!(
        resolution_table.resolution_table.get(&tail_original_id),
        Some(VersionConstraint::Exact(31, id)) if *id == tail_new_id
    ));
}
