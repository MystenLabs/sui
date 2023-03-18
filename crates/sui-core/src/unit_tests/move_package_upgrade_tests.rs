// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{account_address::AccountAddress, ident_str};
use sui_framework_build::compiled_package::BuildConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    messages::{
        Argument, CommandArgumentError, ExecutionFailureStatus, ObjectArg, PackageUpgradeError,
        ProgrammableTransaction, TransactionEffects,
    },
    move_package::{UPGRADE_POLICY_COMPATIBLE, UPGRADE_POLICY_DEP_ONLY},
    object::{Object, Owner},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::BackingPackageStore,
    MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID,
};

use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use crate::authority::{
    authority_tests::{execute_programmable_transaction, init_state},
    move_integration_tests::build_and_publish_test_package_with_upgrade_cap,
    AuthorityState,
};

macro_rules! move_call {
    {$builder:expr, ($addr:expr)::$module_name:ident::$func:ident($($args:expr),* $(,)?)} => {
        $builder.programmable_move_call(
            $addr,
            ident_str!(stringify!($module_name)).to_owned(),
            ident_str!(stringify!($func)).to_owned(),
            vec![],
            vec![$($args),*],
        )
    }
}

pub fn build_upgrade_test_modules(test_dir: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
    let build_config = BuildConfig::new_for_testing();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "move_upgrade", test_dir]);
    let with_unpublished_deps = false;
    let package = sui_framework::build_move_package(&path, build_config).unwrap();
    (
        package.get_package_digest(with_unpublished_deps).to_vec(),
        package.get_package_bytes(with_unpublished_deps),
    )
}

pub fn build_upgrade_test_modules_with_dep_addr(
    test_dir: &str,
    dep_ids: impl IntoIterator<Item = (&'static str, ObjectID)>,
) -> (Vec<u8>, Vec<Vec<u8>>, Vec<ObjectID>) {
    let mut build_config = BuildConfig::new_for_testing();
    for (addr_name, obj_id) in dep_ids.into_iter() {
        build_config
            .config
            .additional_named_addresses
            .insert(addr_name.to_string(), AccountAddress::from(obj_id));
    }
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "move_upgrade", test_dir]);
    let with_unpublished_deps = false;
    let package = sui_framework::build_move_package(&path, build_config).unwrap();
    (
        package.get_package_digest(with_unpublished_deps).to_vec(),
        package.get_package_bytes(with_unpublished_deps),
        package.get_dependency_original_package_ids(),
    )
}

pub struct UpgradeStateRunner {
    pub sender: SuiAddress,
    pub sender_key: AccountKeyPair,
    pub gas_object_id: ObjectID,
    pub authority_state: Arc<AuthorityState>,
    pub package: ObjectRef,
    pub upgrade_cap: ObjectRef,
}

impl UpgradeStateRunner {
    pub async fn new(base_package_name: &str) -> Self {
        let _dont_remove = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_package_upgrades_for_testing(true);
            config
        });
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas_object_id = ObjectID::random();
        let gas_object = Object::with_id_owner_gas_for_testing(gas_object_id, sender, 100000);
        let authority_state = init_state().await;
        authority_state.insert_genesis_object(gas_object).await;

        let (package, upgrade_cap) = build_and_publish_test_package_with_upgrade_cap(
            &authority_state,
            &sender,
            &sender_key,
            &gas_object_id,
            base_package_name,
            /* with_unpublished_deps */ false,
        )
        .await;

        Self {
            sender,
            sender_key,
            gas_object_id,
            authority_state,
            package,
            upgrade_cap,
        }
    }

    pub async fn publish(
        &self,
        modules: Vec<Vec<u8>>,
        dep_ids: Vec<ObjectID>,
    ) -> (ObjectRef, ObjectRef) {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let cap = builder.publish_upgradeable(modules, dep_ids);
            builder.transfer_arg(self.sender, cap);
            builder.finish()
        };
        let TransactionEffects::V1(effects) = self.run(pt).await;
        assert!(effects.status.is_ok());

        let package = effects
            .created
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::Immutable))
            .unwrap();

        let cap = effects
            .created
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
            .unwrap();

        (package.0, cap.0)
    }

    pub async fn run(&self, pt: ProgrammableTransaction) -> TransactionEffects {
        execute_programmable_transaction(
            &self.authority_state,
            &self.gas_object_id,
            &self.sender,
            &self.sender_key,
            pt,
        )
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn test_upgrade_package_happy_path() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");

        // We take as input the upgrade cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };
    let TransactionEffects::V1(effects) = runner.run(pt).await;

    assert!(effects.status.is_ok());
    let new_package = effects
        .created
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap();
    let package = runner
        .authority_state
        .database
        .get_package(&new_package.0 .0)
        .unwrap()
        .unwrap();
    let normalized_modules = package.normalize().unwrap();
    assert!(normalized_modules.contains_key("new_module"));
    assert!(normalized_modules["new_module"]
        .exposed_functions
        .contains_key(ident_str!("this_is_a_new_module")));
    assert!(normalized_modules["new_module"]
        .exposed_functions
        .contains_key(ident_str!(
            "i_can_call_funs_in_other_modules_that_already_existed"
        )));
}

// TODO(tzakian): turn this test on once the Move loader changes.
#[tokio::test]
#[ignore]
async fn test_upgrade_incompatible() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (digest, modules) = build_upgrade_test_modules("compatibility_invalid");

        // We take as input the upgrade cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };
    let TransactionEffects::V1(effects) = runner.run(pt).await;

    assert!(effects.status.is_ok());
}

#[tokio::test]
async fn test_upgrade_package_incorrect_digest() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let (pt, actual_digest) = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
        let bad_digest: Vec<u8> = digest.iter().map(|_| 0).collect();

        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(bad_digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        (builder.finish(), digest)
    };

    let TransactionEffects::V1(effects) = runner.run(pt).await;

    assert_eq!(
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::DigestDoesNotMatch {
                digest: actual_digest
            }
        }
    );
}

#[tokio::test]
async fn test_upgrade_package_compatibility_too_permissive() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");

        // We take as input the upgrade runner.upgrade_cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::only_dep_upgrades(Argument::Input(0))
        };
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        builder.finish()
    };

    let TransactionEffects::V1(effects) = runner.run(pt).await;

    // ETooPermissive abort when we try to authorize the upgrade.
    assert!(matches!(
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::MoveAbort(_, 1)
    ));
}

#[tokio::test]
async fn test_upgrade_package_unsupported_compatibility() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");

        // We take as input the upgrade runner.upgrade_cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_DEP_ONLY).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        builder.finish()
    };

    let TransactionEffects::V1(effects) = runner.run(pt).await;

    // An error currently because we only support compatible upgrades
    assert_eq!(
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::FeatureNotYetSupported
    );
}

#[tokio::test]
async fn test_upgrade_package_invalid_compatibility() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");

        // We take as input the upgrade runner.upgrade_cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();

        // Create the upgrade ticket -- but it has an invalid compatibility policy.
        let upgrade_arg = builder.pure(255u8).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        builder.finish()
    };

    let TransactionEffects::V1(effects) = runner.run(pt).await;

    assert!(matches!(
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::UnknownUpgradePolicy { policy: 255 }
        }
    ));
}

#[tokio::test]
async fn test_upgrade_package_not_a_ticket() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (_, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");

        // We take as input the upgrade runner.upgrade_cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();
        builder.upgrade(current_package_id, Argument::Input(0), vec![], modules);
        builder.finish()
    };
    let TransactionEffects::V1(effects) = runner.run(pt).await;

    assert_eq!(
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::CommandArgumentError {
            arg_idx: 0,
            kind: CommandArgumentError::TypeMismatch
        }
    );
}

#[tokio::test]
async fn test_upgrade_ticket_doesnt_match() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
        // We take as input the upgrade runner.upgrade_cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();
        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        builder.upgrade(MOVE_STDLIB_OBJECT_ID, upgrade_ticket, vec![], modules);
        builder.finish()
    };
    let TransactionEffects::V1(effects) = runner.run(pt).await;

    assert!(matches!(
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::PackageIDDoesNotMatch {
                package_id: _,
                ticket_id: _
            }
        }
    ));
}

#[tokio::test]
async fn upgrade_missing_deps() {
    let TransactionEffects::V1(effects) = test_multiple_upgrades(true).await;
    assert!(matches!(
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::DigestDoesNotMatch { digest: _ }
        }
    ));
}

#[tokio::test]
async fn test_multiple_upgrades_valid() {
    let TransactionEffects::V1(effects) = test_multiple_upgrades(false).await;
    assert!(effects.status.is_ok());
}

async fn test_multiple_upgrades(use_empty_deps: bool) -> TransactionEffects {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt1 = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");

        // We take as input the upgrade cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };
    let TransactionEffects::V1(effects) = runner.run(pt1).await;
    assert!(effects.status.is_ok());

    let new_package = effects
        .created
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap();

    let new_upgrade_cap = effects
        .mutated
        .iter()
        .find(|(obj, _)| obj.0 == runner.upgrade_cap.0)
        .unwrap();

    // Second upgrade: Also adds a dep on the sui framework and stdlib.
    let pt2 = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = new_package.0 .0;
        let (digest, modules) = build_upgrade_test_modules("stage2_basic_compatibility_valid");

        // We take as input the upgrade cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(new_upgrade_cap.0))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let deps = if use_empty_deps {
            vec![]
        } else {
            vec![MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID]
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, deps, modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };
    runner.run(pt2).await
}

// TODO(tzakian): turn this test on once the Move loader changes.
#[tokio::test]
#[ignore]
async fn test_interleaved_upgrades() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;

    // Base has been published. Publish a package now that depends on the base package.
    let (_, module_bytes, dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_upgrading_package",
        [("base_addr", runner.package.0)],
    );
    let (depender_package, depender_cap) = runner.publish(module_bytes, dep_ids).await;

    // publish dependency at version 2
    let pt1 = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");

        // We take as input the upgrade cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };
    let TransactionEffects::V1(effects) = runner.run(pt1).await;
    assert!(effects.status.is_ok());

    let dep_v2_package = effects
        .created
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0;

    let pt2 = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = depender_package.0;
        // Now recompile the depending package with the upgraded dependency
        // Currently doesn't work -- need to wait for linkage table to be added to the loader.
        let (digest, modules, dep_ids) = build_upgrade_test_modules_with_dep_addr(
            "dep_on_upgrading_package",
            [("base_addr", dep_v2_package.0)],
        );

        // We take as input the upgrade cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(depender_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, dep_ids, modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };
    let TransactionEffects::V1(effects) = runner.run(pt2).await;
    assert!(effects.status.is_ok());
}

#[tokio::test]
async fn test_publish_override_happy_path() {
    let runner = UpgradeStateRunner::new("move_upgrade/base").await;

    // Base has been published already. Publish a package now that depends on the base package.
    let (_, module_bytes, dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_upgrading_package",
        [("base_addr", runner.package.0)],
    );
    // Dependency graph: base <-- dep_on_upgrading_package
    let (depender_package, _) = runner.publish(module_bytes, dep_ids).await;

    // publish base package at version 2
    // Dependency graph: base(v1) <-- dep_on_upgrading_package
    //                   base(v2)
    let pt1 = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");

        // We take as input the upgrade cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UPGRADE_POLICY_COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_OBJECT_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };
    let TransactionEffects::V1(effects) = runner.run(pt1).await;
    assert!(effects.status.is_ok());

    let dep_v2_package = effects
        .created
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0;

    // Publish P that depends on both `dep_on_upgrading_package` and `stage1_basic_compatibility_valid`
    // Dependency graph for dep_on_dep:
    //    base(v1)
    //    base(v2) <-- dep_on_upgrading_package <-- dep_on_dep
    let (_, modules, dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_dep",
        [
            ("base_addr", dep_v2_package.0),
            ("dep_on_upgrading_package", depender_package.0),
        ],
    );

    let (new_package, _) = runner.publish(modules, dep_ids).await;

    let package = runner
        .authority_state
        .database
        .get_package(&new_package.0)
        .unwrap()
        .unwrap();

    // Make sure the linkage table points to the correct versions!
    let dep_ids_in_linkage_table: BTreeSet<_> = package
        .linkage_table()
        .values()
        .map(|up| up.upgraded_id)
        .collect();
    assert!(dep_ids_in_linkage_table.contains(&dep_v2_package.0));
    assert!(dep_ids_in_linkage_table.contains(&depender_package.0));
}
