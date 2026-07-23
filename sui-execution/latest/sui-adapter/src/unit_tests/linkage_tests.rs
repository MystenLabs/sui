// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::{PackageMetadata, PackageStore},
    static_programmable_transactions::linkage::{
        config::{LinkageConfig, ResolutionConfig},
        facts::LinkageCommandFacts,
        resolution::ResolutionTable,
        single_linkage::collect_non_type_original_ids,
    },
};
use move_binary_format::file_format::Visibility;
use move_core_types::{account_address::AccountAddress, identifier::IdentStr};
use move_vm_runtime::shared::types::{OriginalId, VersionId};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use sui_protocol_config::{Amendments, ProtocolConfig};
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, SuiResult},
};

#[derive(Clone)]
struct TestPackage {
    version: u64,
    version_id: ObjectID,
    original_id: ObjectID,
    linkage: BTreeMap<OriginalId, VersionId>,
}

impl PackageMetadata for TestPackage {
    fn version(&self) -> u64 {
        self.version
    }

    fn version_id(&self) -> ObjectID {
        self.version_id
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
        _: ObjectID,
        _: &IdentStr,
        _: &IdentStr,
    ) -> SuiResult<Option<ObjectID>> {
        Ok(None)
    }
}

#[test]
fn collects_only_non_type_original_ids() {
    let call_version = ObjectID::from_single_byte(1);
    let call_original = ObjectID::from_single_byte(2);
    let dependency_version = ObjectID::from_single_byte(3);
    let dependency_original = ObjectID::from_single_byte(4);
    let amendment_version = ObjectID::from_single_byte(5);
    let amendment_original = ObjectID::from_single_byte(6);
    let publish_dependency_original = ObjectID::from_single_byte(7);
    let upgrade_dependency_original = ObjectID::from_single_byte(8);
    let upgrade_target_original = ObjectID::from_single_byte(9);
    let type_original = ObjectID::from_single_byte(10);

    let store = TestStore(BTreeMap::from([(
        call_version,
        TestPackage {
            version: 1,
            version_id: call_version,
            original_id: call_original,
            linkage: BTreeMap::from([(dependency_original.into(), dependency_version.into())]),
        },
    )]));
    let amendments: Amendments = BTreeMap::from([(
        AccountAddress::from(call_version),
        BTreeMap::from([(amendment_original.into(), amendment_version.into())]),
    )]);
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    let resolution_table = ResolutionTable::empty(ResolutionConfig::new(
        LinkageConfig::new(Some(Arc::new(amendments)), false),
        protocol_config.binary_config(None).clone(),
    ));
    let mut original_ids = BTreeSet::new();

    for facts in [
        LinkageCommandFacts::MoveCall {
            package: call_version,
            visibility: Visibility::Public,
            type_defining_ids: vec![type_original],
        },
        LinkageCommandFacts::Publish {
            has_init: false,
            linkage: BTreeMap::from([(publish_dependency_original, dependency_version)]),
        },
        LinkageCommandFacts::Upgrade {
            current_package_id: upgrade_target_original,
            current_module_inits: BTreeMap::new(),
            new_modules: vec![],
            linkage: BTreeMap::from([(upgrade_dependency_original, dependency_version)]),
        },
        LinkageCommandFacts::MakeMoveVec {
            type_defining_ids: vec![type_original],
        },
    ] {
        collect_non_type_original_ids::<ExecutionError, _>(
            &facts,
            &resolution_table,
            &store,
            &mut original_ids,
        )
        .unwrap();
    }

    assert_eq!(
        original_ids,
        BTreeSet::from([
            call_original,
            dependency_original,
            amendment_original,
            publish_dependency_original,
            upgrade_dependency_original,
        ])
    );
}
