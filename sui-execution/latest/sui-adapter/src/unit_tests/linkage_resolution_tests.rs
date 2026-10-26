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
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
};
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

fn package_id(id_order: u8, version: u8) -> ObjectID {
    let mut bytes = [0; ObjectID::LENGTH];
    bytes[ObjectID::LENGTH - 3] = 1;
    bytes[ObjectID::LENGTH - 2] = version;
    bytes[ObjectID::LENGTH - 1] = id_order;
    ObjectID::from_bytes(bytes).unwrap()
}

impl PackageMetadata for TestPackage {
    fn version(&self) -> u64 {
        let bytes = self.id.into_bytes();
        if bytes[ObjectID::LENGTH - 3] == 1 {
            u64::from(bytes[ObjectID::LENGTH - 2])
        } else {
            u64::from(bytes[ObjectID::LENGTH - 1])
        }
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

struct CountingStore<'a> {
    store: &'a TestStore,
    package_reads: Cell<usize>,
}

impl PackageStore for CountingStore<'_> {
    type Package = TestPackage;

    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Self::Package>> {
        self.package_reads.set(self.package_reads.get() + 1);
        self.store.get_package(id)
    }

    fn resolve_type_to_defining_id(
        &self,
        module_address: ObjectID,
        module_name: &IdentStr,
        type_name: &IdentStr,
    ) -> SuiResult<Option<ObjectID>> {
        self.store
            .resolve_type_to_defining_id(module_address, module_name, type_name)
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
    let original_id = ObjectID::from_single_byte(50);
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
    let mut resolution_table = ResolutionTable::empty(config);
    resolution_table
        .resolution_table
        .insert(original_id, VersionConstraint::AtLeast(12, selected_id));
    resolution_table.resolution_table.insert(
        selected_dependency,
        VersionConstraint::Exact(100, selected_dependency),
    );
    let resolver = LinkageStoreResolver::<_, ExecutionError>::new(&store, None);
    let mut original_ids = BTreeSet::new();

    resolver
        .collect_original_ids(&resolution_table, historical_id, &mut original_ids)
        .unwrap();

    assert_eq!(
        original_ids,
        BTreeSet::from([original_id, selected_dependency])
    );
}

#[test]
fn executable_package_collection_follows_final_dependency_selection() {
    let root_original_id = ObjectID::from_single_byte(50);
    let root_id = package_id(10, 1);
    let dependency_original_id = ObjectID::from_single_byte(60);
    let dependency_historical_id = package_id(20, 1);
    let dependency_selected_id = package_id(21, 2);
    let new_dependency_original_id = ObjectID::from_single_byte(70);
    let new_dependency_id = package_id(30, 1);
    let store = TestStore(BTreeMap::from([
        (
            root_id,
            TestPackage {
                id: root_id,
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
                linkage: BTreeMap::from([(
                    new_dependency_original_id.into(),
                    new_dependency_id.into(),
                )]),
            },
        ),
        (
            new_dependency_id,
            TestPackage {
                id: new_dependency_id,
                original_id: new_dependency_original_id,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let mut resolution_table = ResolutionTable::empty(config);
    resolution_table
        .resolution_table
        .insert(root_original_id, VersionConstraint::Exact(1, root_id));
    resolution_table.resolution_table.insert(
        dependency_original_id,
        VersionConstraint::AtLeast(2, dependency_selected_id),
    );
    resolution_table.resolution_table.insert(
        new_dependency_original_id,
        VersionConstraint::AtLeast(1, new_dependency_id),
    );
    let resolver = LinkageStoreResolver::<_, ExecutionError>::new(&store, None);
    let mut original_ids = BTreeSet::new();

    resolver
        .collect_original_ids(&resolution_table, root_id, &mut original_ids)
        .unwrap();

    assert_eq!(
        original_ids,
        BTreeSet::from([
            root_original_id,
            dependency_original_id,
            new_dependency_original_id,
        ])
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
    let resolver =
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
fn selected_root_uses_selected_dependency_table() {
    let root_original_id = ObjectID::from_single_byte(1);
    let root_historical_id = package_id(200, 1);
    let root_selected_id = package_id(10, 2);
    let leaf_original_id = ObjectID::from_single_byte(2);
    let leaf_historical_id = package_id(190, 1);
    let leaf_selected_id = package_id(20, 2);
    let store = TestStore(BTreeMap::from([
        (
            root_historical_id,
            TestPackage {
                id: root_historical_id,
                original_id: root_original_id,
                linkage: BTreeMap::from([(leaf_original_id.into(), leaf_historical_id.into())]),
            },
        ),
        (
            root_selected_id,
            TestPackage {
                id: root_selected_id,
                original_id: root_original_id,
                linkage: BTreeMap::from([(leaf_original_id.into(), leaf_selected_id.into())]),
            },
        ),
        (
            leaf_historical_id,
            TestPackage {
                id: leaf_historical_id,
                original_id: leaf_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            leaf_selected_id,
            TestPackage {
                id: leaf_selected_id,
                original_id: leaf_original_id,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok((id == root_original_id).then_some(MinVersion {
            version: 2,
            package_id: ID::new(root_selected_id),
        }))
    };

    for dependency_constraint in [ConstraintKind::Exact, ConstraintKind::AtLeast] {
        let config =
            ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
        let mut resolution_table = ResolutionTable::empty(config);
        let mut resolver =
            LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));
        resolver
            .resolve_package(
                &mut resolution_table,
                root_historical_id,
                ConstraintKind::Exact,
                dependency_constraint,
            )
            .unwrap();

        assert!(matches!(
            resolution_table.resolution_table.get(&root_original_id),
            Some(VersionConstraint::Exact(2, id)) if *id == root_selected_id
        ));
        match dependency_constraint {
            ConstraintKind::Exact => assert!(matches!(
                resolution_table.resolution_table.get(&leaf_original_id),
                Some(VersionConstraint::Exact(2, id)) if *id == leaf_selected_id
            )),
            ConstraintKind::AtLeast => assert!(matches!(
                resolution_table.resolution_table.get(&leaf_original_id),
                Some(VersionConstraint::AtLeast(2, id)) if *id == leaf_selected_id
            )),
        }
    }
}

#[test]
fn selected_flattened_dependency_linkage_is_upward_compatible() {
    for selected_id_order in [5, 250] {
        for (root_leaf_version, selected_leaf_version, succeeds) in [(3, 2, true), (2, 3, false)] {
            let root_original_id = ObjectID::from_single_byte(1);
            let root_id = package_id(100, 1);
            let dependency_original_id = ObjectID::from_single_byte(2);
            let dependency_historical_id = package_id(110, 1);
            let dependency_selected_id = package_id(selected_id_order, 2);
            let leaf_original_id = ObjectID::from_single_byte(3);
            let leaf_v2_id = package_id(120, 2);
            let leaf_v3_id = package_id(130, 3);
            let root_leaf_id = if root_leaf_version == 2 {
                leaf_v2_id
            } else {
                leaf_v3_id
            };
            let selected_leaf_id = if selected_leaf_version == 2 {
                leaf_v2_id
            } else {
                leaf_v3_id
            };
            let store = TestStore(BTreeMap::from([
                (
                    root_id,
                    TestPackage {
                        id: root_id,
                        original_id: root_original_id,
                        linkage: BTreeMap::from([
                            (
                                dependency_original_id.into(),
                                dependency_historical_id.into(),
                            ),
                            (leaf_original_id.into(), root_leaf_id.into()),
                        ]),
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
                        linkage: BTreeMap::from([(
                            leaf_original_id.into(),
                            selected_leaf_id.into(),
                        )]),
                    },
                ),
                (
                    leaf_v2_id,
                    TestPackage {
                        id: leaf_v2_id,
                        original_id: leaf_original_id,
                        linkage: BTreeMap::new(),
                    },
                ),
                (
                    leaf_v3_id,
                    TestPackage {
                        id: leaf_v3_id,
                        original_id: leaf_original_id,
                        linkage: BTreeMap::new(),
                    },
                ),
            ]));
            let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
                Ok((id == dependency_original_id).then_some(MinVersion {
                    version: 2,
                    package_id: ID::new(dependency_selected_id),
                }))
            };
            let config =
                ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
            let mut resolution_table = ResolutionTable::empty(config);
            let mut resolver =
                LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));
            let result = resolver.resolve_package(
                &mut resolution_table,
                root_id,
                ConstraintKind::Exact,
                ConstraintKind::Exact,
            );

            if succeeds {
                result.unwrap();
                assert!(matches!(
                    resolution_table.resolution_table.get(&leaf_original_id),
                    Some(VersionConstraint::Exact(3, id)) if *id == leaf_v3_id
                ));
            } else {
                assert_eq!(
                    result.unwrap_err().kind(),
                    &ExecutionErrorKind::InvalidLinkage
                );
            }
        }
    }
}

#[test]
fn explicit_newer_package_uses_its_own_dependency_table() {
    let root_original_id = ObjectID::from_single_byte(1);
    let root_selected_id = package_id(200, 2);
    let root_newer_id = package_id(10, 3);
    let leaf_original_id = ObjectID::from_single_byte(2);
    let leaf_v2_id = package_id(190, 2);
    let leaf_v3_id = package_id(20, 3);
    let store = TestStore(BTreeMap::from([
        (
            root_selected_id,
            TestPackage {
                id: root_selected_id,
                original_id: root_original_id,
                linkage: BTreeMap::from([(leaf_original_id.into(), leaf_v2_id.into())]),
            },
        ),
        (
            root_newer_id,
            TestPackage {
                id: root_newer_id,
                original_id: root_original_id,
                linkage: BTreeMap::from([(leaf_original_id.into(), leaf_v3_id.into())]),
            },
        ),
        (
            leaf_v2_id,
            TestPackage {
                id: leaf_v2_id,
                original_id: leaf_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            leaf_v3_id,
            TestPackage {
                id: leaf_v3_id,
                original_id: leaf_original_id,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        Ok((id == root_original_id).then_some(MinVersion {
            version: 2,
            package_id: ID::new(root_selected_id),
        }))
    };
    let config = ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
    let mut resolution_table = ResolutionTable::empty(config);
    let mut resolver =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

    resolver
        .resolve_package(
            &mut resolution_table,
            root_newer_id,
            ConstraintKind::Exact,
            ConstraintKind::AtLeast,
        )
        .unwrap();

    assert!(matches!(
        resolution_table.resolution_table.get(&root_original_id),
        Some(VersionConstraint::Exact(3, id)) if *id == root_newer_id
    ));
    assert!(matches!(
        resolution_table.resolution_table.get(&leaf_original_id),
        Some(VersionConstraint::AtLeast(3, id)) if *id == leaf_v3_id
    ));
}

#[test]
fn minversion_is_read_once_per_package_family() {
    let original_id = ObjectID::from_single_byte(1);
    let historical_id = package_id(200, 1);
    let selected_id = package_id(10, 2);
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
    let reads = Cell::new(0);
    let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
        reads.set(reads.get() + 1);
        Ok((id == original_id).then_some(MinVersion {
            version: 2,
            package_id: ID::new(selected_id),
        }))
    };
    let mut resolver =
        LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

    assert_eq!(
        resolver.load_package(&historical_id).unwrap().version_id(),
        selected_id
    );
    assert_eq!(
        resolver.load_package(&selected_id).unwrap().version_id(),
        selected_id
    );
    assert_eq!(reads.get(), 1);
}

#[test]
fn publication_versions_use_only_effective_flattened_linkage() {
    let dependency_original_id = ObjectID::from_single_byte(1);
    let dependency_id = package_id(200, 2);
    let leaf_original_id = ObjectID::from_single_byte(2);
    let stale_leaf_id = package_id(190, 1);
    let effective_leaf_id = package_id(10, 2);
    let store = TestStore(BTreeMap::from([
        (
            dependency_id,
            TestPackage {
                id: dependency_id,
                original_id: dependency_original_id,
                linkage: BTreeMap::from([(leaf_original_id.into(), stale_leaf_id.into())]),
            },
        ),
        (
            stale_leaf_id,
            TestPackage {
                id: stale_leaf_id,
                original_id: leaf_original_id,
                linkage: BTreeMap::new(),
            },
        ),
        (
            effective_leaf_id,
            TestPackage {
                id: effective_leaf_id,
                original_id: leaf_original_id,
                linkage: BTreeMap::new(),
            },
        ),
    ]));
    let mut versions = BTreeSet::new();
    let resolver = LinkageStoreResolver::<_, ExecutionError>::new(&store, None);

    resolver
        .collect_declared_package_versions([dependency_id, effective_leaf_id], &mut versions)
        .unwrap();

    assert_eq!(
        versions,
        BTreeSet::from([(dependency_original_id, 2), (leaf_original_id, 2)])
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
fn selected_cyclic_linkage_is_rejected() {
    let root_original_id = package_id(1, 100);
    let root_historical_id = ObjectID::from_single_byte(10);
    let root_selected_id = ObjectID::from_single_byte(12);
    let dependency_original_id = package_id(2, 100);
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
    for dependency_constraint in [ConstraintKind::Exact, ConstraintKind::AtLeast] {
        let config =
            ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
        let mut resolution_table = ResolutionTable::empty(config);
        let mut resolver =
            LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

        resolver
            .resolve_package(
                &mut resolution_table,
                root_historical_id,
                ConstraintKind::Exact,
                dependency_constraint,
            )
            .unwrap();

        assert_eq!(resolution_table.resolution_table.len(), 2);
        assert_eq!(
            resolver
                .validate_acyclic_linkage(&resolution_table)
                .unwrap_err()
                .kind(),
            &ExecutionErrorKind::InvalidLinkage
        );
    }
}

#[test]
fn cross_version_cycles_without_minversion_are_rejected_regardless_of_id_order() {
    for a_original_first in [true, false] {
        for a_selected_first in [true, false] {
            let (a_original_id, b_original_id) = if a_original_first {
                (ObjectID::from_single_byte(1), ObjectID::from_single_byte(2))
            } else {
                (ObjectID::from_single_byte(2), ObjectID::from_single_byte(1))
            };
            let (a_order, b_order) = if a_selected_first { (1, 2) } else { (2, 1) };
            let a_v1_id = package_id(a_order, 1);
            let a_v2_id = package_id(a_order, 2);
            let b_v1_id = package_id(b_order, 1);
            let b_v2_id = package_id(b_order, 2);
            let store = TestStore(BTreeMap::from([
                (
                    a_v1_id,
                    TestPackage {
                        id: a_v1_id,
                        original_id: a_original_id,
                        linkage: BTreeMap::new(),
                    },
                ),
                (
                    a_v2_id,
                    TestPackage {
                        id: a_v2_id,
                        original_id: a_original_id,
                        linkage: BTreeMap::from([(b_original_id.into(), b_v1_id.into())]),
                    },
                ),
                (
                    b_v1_id,
                    TestPackage {
                        id: b_v1_id,
                        original_id: b_original_id,
                        linkage: BTreeMap::new(),
                    },
                ),
                (
                    b_v2_id,
                    TestPackage {
                        id: b_v2_id,
                        original_id: b_original_id,
                        linkage: BTreeMap::from([(a_original_id.into(), a_v1_id.into())]),
                    },
                ),
            ]));
            let config =
                ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
            let mut resolution_table = ResolutionTable::empty(config);
            let mut resolver = LinkageStoreResolver::<_, ExecutionError>::new(&store, None);
            for package_id in [a_v2_id, b_v2_id] {
                resolver
                    .resolve_package(
                        &mut resolution_table,
                        package_id,
                        ConstraintKind::AtLeast,
                        ConstraintKind::AtLeast,
                    )
                    .unwrap();
            }

            let counting_store = CountingStore {
                store: &store,
                package_reads: Cell::new(0),
            };
            let resolver = LinkageStoreResolver::<_, ExecutionError>::new(&counting_store, None);
            assert_eq!(
                resolver
                    .validate_acyclic_linkage(&resolution_table)
                    .unwrap_err()
                    .kind(),
                &ExecutionErrorKind::InvalidLinkage
            );
            // Validation loads each selected package once, but obtains dependency original IDs
            // directly from linkage-table keys rather than loading the dependency versions.
            assert_eq!(counting_store.package_reads.get(), 2);
        }
    }
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
