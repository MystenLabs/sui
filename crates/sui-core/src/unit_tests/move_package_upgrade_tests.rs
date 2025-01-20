// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{ident_str, language_storage::StructTag};
use sui_move_build::BuildConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    move_package::UpgradePolicy,
    object::{Object, Owner},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::ObjectStore,
    transaction::{Argument, ObjectArg, ProgrammableTransaction, TEST_ONLY_GAS_UNIT_FOR_PUBLISH},
    MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID,
};

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::error::{SuiError, UserInputError};
use sui_types::execution_config_utils::to_binary_config;
use sui_types::execution_status::{
    CommandArgumentError, ExecutionFailureStatus, ExecutionStatus, PackageUpgradeError,
};

use crate::authority::authority_tests::init_state_with_ids;
use crate::authority::move_integration_tests::{
    build_multi_publish_txns, build_multi_upgrade_txns, build_package,
    collect_packages_and_upgrade_caps, run_multi_txns, UpgradeData,
};
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::{
    auth_unit_test_utils::build_test_modules_with_dep_addr,
    authority_tests::execute_programmable_transaction,
    move_integration_tests::build_and_publish_test_package_with_upgrade_cap, AuthorityState,
};

#[macro_export]
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

enum FileOverlay<'a> {
    Remove(&'a str),
    Add {
        file_name: &'a str,
        contents: &'a str,
    },
}

fn build_upgrade_test_modules_with_overlay(
    base_pkg: &str,
    overlay: FileOverlay<'_>,
) -> (Vec<u8>, Vec<Vec<u8>>) {
    // Root temp dirs under `move_upgrade` directory so that dependency paths remain correct.
    let mut tmp_dir_root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    tmp_dir_root_path.extend(["src", "unit_tests", "data", "move_upgrade"]);

    let tmp_dir = tempfile::TempDir::new_in(tmp_dir_root_path).unwrap();
    let tmp_dir_path = tmp_dir.path();

    let mut copy_options = fs_extra::dir::CopyOptions::new();
    copy_options.copy_inside = true;
    copy_options.content_only = true;
    let source_dir = pkg_path_of(base_pkg);
    fs_extra::dir::copy(source_dir, tmp_dir_path, &copy_options).unwrap();

    match overlay {
        FileOverlay::Remove(file_name) => {
            let file_path = tmp_dir_path.join(format!("sources/{}", file_name));
            std::fs::remove_file(file_path).unwrap();
        }
        FileOverlay::Add {
            file_name,
            contents,
        } => {
            let new_file_path = tmp_dir_path.join(format!("sources/{}", file_name));
            std::fs::write(new_file_path, contents).unwrap();
        }
    }

    build_pkg_at_path(tmp_dir_path)
}

fn build_upgrade_test_modules(test_dir: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
    let path = pkg_path_of(test_dir);
    build_pkg_at_path(&path)
}

fn pkg_path_of(pkg_name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "move_upgrade", pkg_name]);
    path
}

fn build_pkg_at_path(path: &Path) -> (Vec<u8>, Vec<Vec<u8>>) {
    let with_unpublished_deps = false;
    let package = BuildConfig::new_for_testing().build(path).unwrap();
    (
        package.get_package_digest(with_unpublished_deps).to_vec(),
        package.get_package_bytes(with_unpublished_deps),
    )
}

pub fn build_upgrade_test_modules_with_dep_addr(
    test_dir: &str,
    dep_original_addresses: impl IntoIterator<Item = (&'static str, ObjectID)>,
    dep_ids: impl IntoIterator<Item = (&'static str, ObjectID)>,
) -> (Vec<u8>, Vec<Vec<u8>>, Vec<ObjectID>) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "move_upgrade", test_dir]);
    let package = build_test_modules_with_dep_addr(&path, dep_original_addresses, dep_ids);
    let with_unpublished_deps = false;
    (
        package.get_package_digest(with_unpublished_deps).to_vec(),
        package.get_package_bytes(with_unpublished_deps),
        package.dependency_ids.published.values().cloned().collect(),
    )
}

pub fn build_upgrade_txn(
    current_pkg_id: ObjectID,
    upgraded_pkg_name: &str,
    upgrade_cap: ObjectRef,
) -> ProgrammableTransaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    let (digest, modules) = build_upgrade_test_modules(upgraded_pkg_name);

    // We take as input the upgrade cap
    builder
        .obj(ObjectArg::ImmOrOwnedObject(upgrade_cap))
        .unwrap();

    // Create the upgrade ticket
    let upgrade_arg = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
    let digest_arg = builder.pure(digest).unwrap();
    let upgrade_ticket = move_call! {
        builder,
        (SUI_FRAMEWORK_PACKAGE_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
    };
    let upgrade_receipt = builder.upgrade(current_pkg_id, upgrade_ticket, vec![], modules);
    move_call! {
        builder,
        (SUI_FRAMEWORK_PACKAGE_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
    };

    builder.finish()
}

struct UpgradeStateRunner {
    pub sender: SuiAddress,
    pub sender_key: AccountKeyPair,
    pub gas_object_id: ObjectID,
    pub authority_state: Arc<AuthorityState>,
    pub package: ObjectRef,
    pub upgrade_cap: ObjectRef,
    pub rgp: u64,
}

impl UpgradeStateRunner {
    pub async fn new(base_package_name: &str) -> Self {
        telemetry_subscribers::init_for_testing();
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas_object_id = ObjectID::random();
        let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
        let authority_state = TestAuthorityBuilder::new().build().await;
        authority_state.insert_genesis_object(gas_object).await;
        let rgp = authority_state.reference_gas_price_for_testing().unwrap();

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
            rgp,
        }
    }

    pub async fn publish(
        &mut self,
        modules: Vec<Vec<u8>>,
        dep_ids: Vec<ObjectID>,
    ) -> (ObjectRef, ObjectRef) {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let cap = builder.publish_upgradeable(modules, dep_ids);
            builder.transfer_arg(self.sender, cap);
            builder.finish()
        };
        let effects = self.run(pt).await;
        assert!(effects.status().is_ok(), "{:#?}", effects.status());

        let package = effects
            .created()
            .into_iter()
            .find(|(_, owner)| matches!(owner, Owner::Immutable))
            .unwrap();

        let cap = effects
            .created()
            .into_iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
            .unwrap();

        (package.0, cap.0)
    }

    pub async fn upgrade(
        &mut self,
        policy: u8,
        digest: Vec<u8>,
        modules: Vec<Vec<u8>>,
        dep_ids: Vec<ObjectID>,
    ) -> TransactionEffects {
        let pt = {
            let package_id = self.package.0;
            let mut builder = ProgrammableTransactionBuilder::new();

            let cap = builder
                .obj(ObjectArg::ImmOrOwnedObject(self.upgrade_cap))
                .unwrap();
            let policy = builder.pure(policy).unwrap();
            let digest = builder.pure(digest).unwrap();
            let ticket = move_call! {
                builder,
                (SUI_FRAMEWORK_PACKAGE_ID)::package::authorize_upgrade(cap, policy, digest)
            };

            let receipt = builder.upgrade(package_id, ticket, dep_ids, modules);
            move_call! { builder, (SUI_FRAMEWORK_PACKAGE_ID)::package::commit_upgrade(cap, receipt) };

            builder.finish()
        };

        let effects = self.run(pt).await;
        if effects.status().is_ok() {
            self.package = effects
                .created()
                .into_iter()
                .find_map(|(pkg, owner)| matches!(owner, Owner::Immutable).then_some(pkg))
                .unwrap();
        }

        effects
    }

    pub async fn run(&mut self, pt: ProgrammableTransaction) -> TransactionEffects {
        let effects = execute_programmable_transaction(
            &self.authority_state,
            &self.gas_object_id,
            &self.sender,
            &self.sender_key,
            pt,
            self.rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        )
        .await
        .unwrap();

        if let Some(updated_cap) = effects
            .mutated()
            .into_iter()
            .find_map(|(cap, _)| (cap.0 == self.upgrade_cap.0).then_some(cap))
        {
            self.upgrade_cap = updated_cap;
        }

        effects
    }
}

#[tokio::test]
async fn test_upgrade_package_happy_path() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (runner.package.0)::base::return_0()
            };

            builder.finish()
        })
        .await;

    match effects.into_status().unwrap_err().0 {
        ExecutionFailureStatus::MoveAbort(_, 42) => { /* nop */ }
        err => panic!("Unexpected error: {:#?}", err),
    };

    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    let package = runner
        .authority_state
        .get_object_cache_reader()
        .get_package_object(&runner.package.0)
        .unwrap()
        .unwrap();
    let config = ProtocolConfig::get_for_max_version_UNSAFE();
    let binary_config = to_binary_config(&config);
    let normalized_modules = package.move_package().normalize(&binary_config).unwrap();
    assert!(normalized_modules.contains_key("new_module"));
    assert!(normalized_modules["new_module"]
        .functions
        .contains_key(ident_str!("this_is_a_new_module")));
    assert!(normalized_modules["new_module"]
        .functions
        .contains_key(ident_str!(
            "i_can_call_funs_in_other_modules_that_already_existed"
        )));

    // Call into the upgraded module
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (runner.package.0)::base::return_0()
            };

            builder.finish()
        })
        .await;
    assert!(effects.status().is_ok(), "{:#?}", effects.status());
}

#[tokio::test]
async fn test_upgrade_introduces_type_then_uses_it() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    // First upgrade introduces a new type, B.
    let (digest, modules) = build_upgrade_test_modules("new_object");
    let effects = runner
        .upgrade(
            UpgradePolicy::COMPATIBLE,
            digest,
            modules,
            vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
        )
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    let package_v2 = runner.package.0;

    // Second upgrade introduces an entry function that creates `B`s.
    let (digest, modules) = build_upgrade_test_modules("makes_new_object");
    let effects = runner
        .upgrade(
            UpgradePolicy::COMPATIBLE,
            digest,
            modules,
            vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
        )
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    let package_v3 = runner.package.0;

    // Create an instance of the type introduced at version 2, with the function introduced at
    // version 3.
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v3)::base::makes_b() };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    let created = effects
        .created()
        .into_iter()
        .find_map(|(b, owner)| matches!(owner, Owner::AddressOwner(_)).then_some(b))
        .unwrap();

    let b = runner
        .authority_state
        .get_object_store()
        .get_object_by_key(&created.0, created.1)
        .unwrap();

    assert_eq!(
        b.data.struct_tag().unwrap(),
        StructTag {
            address: *package_v2,
            module: ident_str!("base").to_owned(),
            name: ident_str!("B").to_owned(),
            type_params: vec![],
        },
    );

    // Delete the instance we just created
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            let b = builder.obj(ObjectArg::ImmOrOwnedObject(created)).unwrap();
            move_call! { builder, (package_v3)::base::destroys_b(b) };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
}

#[tokio::test]
async fn test_upgrade_incompatible() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("compatibility_invalid");
    let effects = runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    assert_eq!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::IncompatibleUpgrade,
        },
    )
}

#[tokio::test]
async fn test_upgrade_package_incorrect_digest() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    let bad_digest = vec![0; digest.len()];

    let effects = runner
        .upgrade(UpgradePolicy::COMPATIBLE, bad_digest, modules, vec![])
        .await;

    assert_eq!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::DigestDoesNotMatch { digest }
        }
    );
}

#[tokio::test]
async fn test_upgrade_package_compatibility_too_permissive() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            let cap = builder
                .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
                .unwrap();
            move_call! { builder, (SUI_FRAMEWORK_PACKAGE_ID)::package::only_dep_upgrades(cap) };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());

    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    let effects = runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    // ETooPermissive abort when we try to authorize the upgrade.
    assert!(matches!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::MoveAbort(_, 1)
    ));
}

#[tokio::test]
async fn test_upgrade_package_compatible_in_dep_only_mode() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    let effects = runner
        .upgrade(UpgradePolicy::DEP_ONLY, digest, modules, vec![])
        .await;

    assert_eq!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::IncompatibleUpgrade
        },
    );
}

#[tokio::test]
async fn test_upgrade_package_add_new_module_in_dep_only_mode_pre_v68() {
    // Allow new modules in deps-only mode for this test.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_disallow_new_modules_in_deps_only_packages_for_testing(false);
        config
    });

    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let base_pkg = "dep_only_upgrade";
    assert_valid_dep_only_upgrade(&mut runner, base_pkg).await;
    let (digest, modules) = build_upgrade_test_modules_with_overlay(
        base_pkg,
        FileOverlay::Add {
            file_name: "new_module.move",
            contents: "module base_addr::new_module;",
        },
    );
    let effects = runner
        .upgrade(
            UpgradePolicy::DEP_ONLY,
            digest,
            modules,
            vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
        )
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
}

#[tokio::test]
async fn test_upgrade_package_invalid_dep_only_upgrade_pre_v68() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_disallow_new_modules_in_deps_only_packages_for_testing(false);
        config
    });

    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let base_pkg = "dep_only_upgrade";
    assert_valid_dep_only_upgrade(&mut runner, base_pkg).await;
    let overlays = [
        FileOverlay::Add {
            file_name: "new_friend_module.move",
            contents: r#"
module base_addr::new_friend_module;
public fun friend_call(): u64 { base_addr::base::friend_fun(1) }
        "#,
        },
        FileOverlay::Remove("friend_module.move"),
    ];
    for overlay in overlays {
        let (digest, modules) = build_upgrade_test_modules_with_overlay(base_pkg, overlay);
        let effects = runner
            .upgrade(
                UpgradePolicy::DEP_ONLY,
                digest,
                modules,
                vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
            )
            .await;

        assert_eq!(
            effects.into_status().unwrap_err().0,
            ExecutionFailureStatus::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::IncompatibleUpgrade
            },
        );
    }
}

#[tokio::test]
async fn test_invalid_dep_only_upgrades() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let base_pkg = "dep_only_upgrade";
    assert_valid_dep_only_upgrade(&mut runner, base_pkg).await;
    let overlays = [
        FileOverlay::Add {
            file_name: "new_module.move",
            contents: "module base_addr::new_module;",
        },
        FileOverlay::Add {
            file_name: "new_friend_module.move",
            contents: r#"
module base_addr::new_friend_module;
public fun friend_call(): u64 { base_addr::base::friend_fun(1) }
        "#,
        },
        FileOverlay::Remove("friend_module.move"),
    ];

    for overlay in overlays {
        let (digest, modules) = build_upgrade_test_modules_with_overlay(base_pkg, overlay);
        let effects = runner
            .upgrade(
                UpgradePolicy::DEP_ONLY,
                digest,
                modules,
                vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
            )
            .await;

        assert_eq!(
            effects.into_status().unwrap_err().0,
            ExecutionFailureStatus::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::IncompatibleUpgrade
            },
        );
    }
}

#[tokio::test]
async fn test_upgrade_package_compatible_in_additive_mode() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    let effects = runner
        .upgrade(UpgradePolicy::ADDITIVE, digest, modules, vec![])
        .await;

    assert_eq!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::IncompatibleUpgrade
        },
    );
}

#[tokio::test]
async fn test_upgrade_package_invalid_compatibility() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    let effects = runner.upgrade(255u8, digest, modules, vec![]).await;

    assert!(matches!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::UnknownUpgradePolicy { policy: 255 }
        }
    ));
}

#[tokio::test]
async fn test_upgrade_package_missing_type() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/missing_type_v1").await;

    let (digest, modules) = build_upgrade_test_modules("missing_type_v2");
    let effects = runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    assert!(matches!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::IncompatibleUpgrade
        }
    ));
}

#[tokio::test]
async fn test_upgrade_package_missing_type_module_removal() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/missing_type_v1").await;

    let (digest, modules) = build_upgrade_test_modules("missing_type_v2_module_removed");
    let effects = runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    assert!(matches!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::IncompatibleUpgrade
        }
    ));
}

#[tokio::test]
async fn test_upgrade_package_additive_mode() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("additive_upgrade");
    let effects = runner
        .upgrade(UpgradePolicy::ADDITIVE, digest, modules, vec![])
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
}

#[tokio::test]
async fn test_upgrade_package_invalid_additive_mode() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("additive_upgrade_invalid");
    let effects = runner
        .upgrade(UpgradePolicy::ADDITIVE, digest, modules, vec![])
        .await;

    assert_eq!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::IncompatibleUpgrade
        },
    );
}

#[tokio::test]
async fn test_upgrade_package_additive_dep_only_mode() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("additive_upgrade");
    let effects = runner
        .upgrade(UpgradePolicy::DEP_ONLY, digest, modules, vec![])
        .await;

    assert_eq!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::IncompatibleUpgrade
        },
    );
}

#[tokio::test]
async fn test_upgrade_package_dep_only_mode() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    assert_valid_dep_only_upgrade(&mut runner, "dep_only_upgrade").await;
}

#[tokio::test]
async fn test_upgrade_package_not_a_ticket() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = runner.package.0;
        let (_, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");

        // We take as input the upgrade runner.upgrade_cap
        let cap = builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();
        builder.upgrade(current_package_id, cap, vec![], modules);
        builder.finish()
    };
    let effects = runner.run(pt).await;

    assert_eq!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::CommandArgumentError {
            arg_idx: 0,
            kind: CommandArgumentError::TypeMismatch
        }
    );
}

#[tokio::test]
async fn test_upgrade_ticket_doesnt_match() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
        // We take as input the upgrade runner.upgrade_cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
            .unwrap();
        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_PACKAGE_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        builder.upgrade(MOVE_STDLIB_PACKAGE_ID, upgrade_ticket, vec![], modules);
        builder.finish()
    };
    let effects = runner.run(pt).await;

    assert!(matches!(
        effects.into_status().unwrap_err().0,
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
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let (_, effects) = test_multiple_upgrades(&mut runner, true).await;
    assert!(matches!(
        effects.into_status().unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::DigestDoesNotMatch { digest: _ }
        }
    ));
}

#[tokio::test]
async fn test_multiple_upgrades_valid() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let (_, effects) = test_multiple_upgrades(&mut runner, false).await;
    assert!(effects.status().is_ok(), "{:#?}", effects.status());
}

async fn test_multiple_upgrades(
    runner: &mut UpgradeStateRunner,
    use_empty_deps: bool,
) -> (ObjectID, TransactionEffects) {
    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    let effects = runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());

    let package_v2 = effects
        .created()
        .into_iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0
         .0;

    // Second upgrade: May also adds a dep on the sui framework and stdlib.
    let (digest, modules) = build_upgrade_test_modules("stage2_basic_compatibility_valid");
    let effects = runner
        .upgrade(
            UpgradePolicy::COMPATIBLE,
            digest,
            modules,
            if use_empty_deps {
                vec![]
            } else {
                vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID]
            },
        )
        .await;
    (package_v2, effects)
}

#[tokio::test]
async fn test_interleaved_upgrades() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    // Base has been published. Publish a package now that depends on the base package.
    let (_, module_bytes, dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_upgrading_package",
        [("base_addr", runner.package.0)],
        [("package_upgrade_base", runner.package.0)],
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
        let upgrade_arg = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_PACKAGE_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, vec![], modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_PACKAGE_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };
    let effects = runner.run(pt1).await;
    assert!(effects.status().is_ok(), "{:#?}", effects.status());

    let dep_v2_package = effects
        .created()
        .into_iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0;

    let pt2 = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = depender_package.0;
        // Now recompile the depending package with the upgraded dependency
        let (digest, modules, dep_ids) = build_upgrade_test_modules_with_dep_addr(
            "dep_on_upgrading_package",
            [("base_addr", runner.package.0)],
            [("package_upgrade_base", dep_v2_package.0)],
        );

        // We take as input the upgrade cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(depender_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_PACKAGE_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, dep_ids, modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_PACKAGE_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };
    let effects = runner.run(pt2).await;
    assert!(effects.status().is_ok(), "{:#?}", effects.status());
}

#[tokio::test]
async fn test_publish_override_happy_path() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    // Base has been published already. Publish a package now that depends on the base package.
    let (_, module_bytes, dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_upgrading_package",
        [("base_addr", runner.package.0)],
        [("package_upgrade_base", runner.package.0)],
    );
    // Dependency graph: base <-- dep_on_upgrading_package
    let (depender_package, _) = runner.publish(module_bytes, dep_ids).await;

    // publish base package at version 2
    // Dependency graph: base(v1) <-- dep_on_upgrading_package
    //                   base(v2)
    let pt1 = build_upgrade_txn(
        runner.package.0,
        "stage1_basic_compatibility_valid",
        runner.upgrade_cap,
    );

    let effects = runner.run(pt1).await;
    assert!(effects.status().is_ok(), "{:#?}", effects.status());

    let dep_v2_package = effects
        .created()
        .into_iter()
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
        [
            ("package_upgrade_base", dep_v2_package.0),
            ("dep_on_upgrading_package", depender_package.0),
        ],
    );

    let (new_package, _) = runner.publish(modules, dep_ids).await;

    let package = runner
        .authority_state
        .get_object_cache_reader()
        .get_package_object(&new_package.0)
        .unwrap()
        .unwrap();

    // Make sure the linkage table points to the correct versions!
    let dep_ids_in_linkage_table: BTreeSet<_> = package
        .move_package()
        .linkage_table()
        .values()
        .map(|up| up.upgraded_id)
        .collect();
    assert!(dep_ids_in_linkage_table.contains(&dep_v2_package.0));
    assert!(dep_ids_in_linkage_table.contains(&depender_package.0));
}

#[tokio::test]
async fn test_publish_transitive_happy_path() {
    // publishes base package at version 1
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    // publish a package that depends on the base package
    let (_, module_bytes, dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_upgrading_package_upgradeable",
        [
            ("base_addr", runner.package.0),
            ("dep_on_upgrading_package", ObjectID::ZERO),
        ],
        [("package_upgrade_base", runner.package.0)],
    );
    // Dependency graph: base <-- dep_on_upgrading_package
    let (depender_package, _) = runner.publish(module_bytes, dep_ids).await;

    // publish a root package that depends on the dependent package and on version 1 of the base
    // package (both dependent package and transitively dependent package depended on the same
    // version of the base package)
    let (_, root_module_bytes, root_dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_upgrading_package_transitive",
        [
            ("base_addr", runner.package.0),
            ("dep_on_upgrading_package", depender_package.0),
        ],
        [
            ("package_upgrade_base", runner.package.0),
            ("dep_on_upgrading_package", depender_package.0),
        ],
    );
    // Dependency graph: base(v1)  <-- dep_on_upgrading_package
    //                   base(v1)  <-- dep_on_upgrading_package <-- dep_on_upgrading_package_transitive --> base(v1)
    let (root_package, _) = runner.publish(root_module_bytes, root_dep_ids).await;

    let root_move_package = runner
        .authority_state
        .get_object_cache_reader()
        .get_package_object(&root_package.0)
        .unwrap()
        .unwrap();

    // Make sure the linkage table points to the correct versions!
    let dep_ids_in_linkage_table: BTreeSet<_> = root_move_package
        .move_package()
        .linkage_table()
        .values()
        .map(|up| up.upgraded_id)
        .collect();
    assert!(dep_ids_in_linkage_table.contains(&runner.package.0));
    assert!(dep_ids_in_linkage_table.contains(&depender_package.0));

    // Call into the root module to call base module's function (should abort due to base module's
    // call_return_0 aborting with code 42)
    let call_effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (root_package.0)::my_module::call_return_0()
            };

            builder.finish()
        })
        .await;

    match call_effects.into_status().unwrap_err().0 {
        ExecutionFailureStatus::MoveAbort(_, 42) => { /* nop */ }
        err => panic!("Unexpected error: {:#?}", err),
    };
}

#[tokio::test]
async fn test_publish_transitive_override_happy_path() {
    // publishes base package at version 1
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    // publish a package that depends on the base package
    let (_, module_bytes, dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_upgrading_package_upgradeable",
        [
            ("base_addr", runner.package.0),
            ("dep_on_upgrading_package", ObjectID::ZERO),
        ],
        [("package_upgrade_base", runner.package.0)],
    );
    // Dependency graph: base <-- dep_on_upgrading_package
    let (depender_package, _) = runner.publish(module_bytes, dep_ids).await;

    // publish base package at version 2
    let pt1 = build_upgrade_txn(
        runner.package.0,
        "stage1_basic_compatibility_valid",
        runner.upgrade_cap,
    );

    let effects = runner.run(pt1).await;
    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    // Dependency graph: base(v1) <-- dep_on_upgrading_package
    //                   base(v2)

    let base_v2_package = effects
        .created()
        .into_iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0;

    // publish a root package that depends on the dependent package and on version 2 of the base
    // package (overriding base package dependency of the dependent package which originally
    // depended on base package version 1)
    let (_, root_module_bytes, root_dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_upgrading_package_transitive",
        [
            ("base_addr", runner.package.0),
            ("dep_on_upgrading_package", depender_package.0),
        ],
        [
            ("package_upgrade_base", base_v2_package.0),
            ("dep_on_upgrading_package", depender_package.0),
        ],
    );
    // Dependency graph: base(v1)  <-- dep_on_upgrading_package
    //                   base(v2)  <-- dep_on_upgrading_package <-- dep_on_upgrading_package_transitive --> base(v2)
    let (root_package, _) = runner.publish(root_module_bytes, root_dep_ids).await;

    let root_move_package = runner
        .authority_state
        .get_object_cache_reader()
        .get_package_object(&root_package.0)
        .unwrap()
        .unwrap();

    // Make sure the linkage table points to the correct versions!
    let dep_ids_in_linkage_table: BTreeSet<_> = root_move_package
        .move_package()
        .linkage_table()
        .values()
        .map(|up| up.upgraded_id)
        .collect();
    assert!(dep_ids_in_linkage_table.contains(&base_v2_package.0));
    assert!(dep_ids_in_linkage_table.contains(&depender_package.0));

    // Call into the root module to call upgraded base module's function (should succeed due to base module's
    // call_return_0 no longer aborting)
    let call_effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (root_package.0)::my_module::call_return_0()
            };

            builder.finish()
        })
        .await;

    assert!(
        call_effects.status().is_ok(),
        "{:#?}",
        call_effects.status()
    );
}

#[tokio::test]
async fn test_upgraded_types_in_one_txn() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    // First upgrade (version 2) introduces a new type, B.
    let (digest, modules) = build_upgrade_test_modules("makes_new_object");
    let effects = runner
        .upgrade(
            UpgradePolicy::COMPATIBLE,
            digest,
            modules,
            vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
        )
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    let package_v2 = runner.package.0;

    // Second upgrade (version 3) introduces a new type, C.
    let (digest, modules) = build_upgrade_test_modules("makes_another_object");
    let effects = runner
        .upgrade(
            UpgradePolicy::COMPATIBLE,
            digest,
            modules,
            vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
        )
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    let package_v3 = runner.package.0;

    // Create an instance of the type introduced at version 2 using function from version 2.
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v2)::base::makes_b() };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    let created_b = effects
        .created()
        .into_iter()
        .find_map(|(b, owner)| matches!(owner, Owner::AddressOwner(_)).then_some(b))
        .unwrap();

    // Create an instance of the type introduced at version 3 using function from version 3.
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v3)::base::makes_c() };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    let created_c = effects
        .created()
        .into_iter()
        .find_map(|(c, owner)| matches!(owner, Owner::AddressOwner(_)).then_some(c))
        .unwrap();

    // modify objects created of types introduced at versions 2 and 3 and emit events using types
    // introduced at versions 2 and 3 (using functions from version 3)
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            let b = builder.obj(ObjectArg::ImmOrOwnedObject(created_b)).unwrap();
            move_call! { builder, (package_v3)::base::modifies_b(b) };
            let c = builder.obj(ObjectArg::ImmOrOwnedObject(created_c)).unwrap();
            move_call! { builder, (package_v3)::base::modifies_c(c) };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());

    // verify that the types of events match
    let e1_type = StructTag::from_str(&format!("{package_v2}::base::BModEvent")).unwrap();
    let e2_type = StructTag::from_str(&format!("{package_v3}::base::CModEvent")).unwrap();

    let event_digest = effects.events_digest().unwrap();
    let mut events = runner
        .authority_state
        .get_transaction_events(event_digest)
        .unwrap()
        .data;
    events.sort_by(|a, b| a.type_.name.as_str().cmp(b.type_.name.as_str()));
    assert!(events.len() == 2);
    assert_eq!(events[0].type_, e1_type);
    assert_eq!(events[1].type_, e2_type);
}

#[tokio::test]
async fn test_different_versions_across_calls() {
    // create 3 versions of the same package, all containing the return_0 function
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let (package_v2, effects) = test_multiple_upgrades(&mut runner, false).await;
    assert!(effects.status().is_ok(), "{:#?}", effects.status());

    let package_v3 = effects
        .created()
        .into_iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0
         .0;

    // call the same function twice within the same block but from two different module versions
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v2)::base::return_0() };
            move_call! { builder, (package_v3)::base::return_0() };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
}

#[tokio::test]
async fn test_conflicting_versions_across_calls() {
    // publishes base package at version 1
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    // publish a dependent package at version 1 that depends on the base package at version 1
    let (_, module_bytes, dep_ids) = build_upgrade_test_modules_with_dep_addr(
        "dep_on_upgrading_package_upgradeable",
        [
            ("base_addr", runner.package.0),
            ("dep_on_upgrading_package", ObjectID::ZERO),
        ],
        [("package_upgrade_base", runner.package.0)],
    );
    let (depender_package, depender_cap) = runner.publish(module_bytes, dep_ids).await;

    // publish base package at version 2
    let pt1 = build_upgrade_txn(
        runner.package.0,
        "stage1_basic_compatibility_valid",
        runner.upgrade_cap,
    );

    let effects = runner.run(pt1).await;
    assert!(effects.status().is_ok(), "{:#?}", effects.status());

    let base_v2_package = effects
        .created()
        .into_iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0;

    // publish a dependent package at version 2 that depends on the base package at version 2
    let pt2 = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let current_package_id = depender_package.0;
        // Now recompile the depending package with the upgraded dependency
        let (digest, modules, dep_ids) = build_upgrade_test_modules_with_dep_addr(
            "dep_on_upgrading_package_upgradeable",
            [
                ("base_addr", runner.package.0),
                ("dep_on_upgrading_package", ObjectID::ZERO),
            ],
            [("package_upgrade_base", base_v2_package.0)],
        );

        // We take as input the upgrade cap
        builder
            .obj(ObjectArg::ImmOrOwnedObject(depender_cap))
            .unwrap();

        // Create the upgrade ticket
        let upgrade_arg = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
        let digest_arg = builder.pure(digest).unwrap();
        let upgrade_ticket = move_call! {
            builder,
            (SUI_FRAMEWORK_PACKAGE_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
        };
        let upgrade_receipt = builder.upgrade(current_package_id, upgrade_ticket, dep_ids, modules);
        move_call! {
            builder,
            (SUI_FRAMEWORK_PACKAGE_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
        };

        builder.finish()
    };

    let effects = runner.run(pt2).await;
    assert!(effects.status().is_ok(), "{:#?}", effects.status());

    let dependent_v2_package = effects
        .created()
        .into_iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0;

    // call the same function twice within the same block but from two different module versions
    // that differ only by having different dependencies
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            // call from upgraded package - should succeed
            move_call! { builder, (dependent_v2_package.0)::my_module::call_return_0() };
            // call from original package - should abort (check later that the second command
            // aborts)
            move_call! { builder, (depender_package.0)::my_module::call_return_0() };
            builder.finish()
        })
        .await;

    let call_error = effects.into_status().unwrap_err();

    // verify that execution aborts
    match call_error.0 {
        ExecutionFailureStatus::MoveAbort(_, 42) => { /* nop */ }
        err => panic!("Unexpected error: {:#?}", err),
    };

    // verify that execution aborts in the second (counting from 0) command
    assert_eq!(call_error.1, Some(1));
}

#[tokio::test]
async fn test_upgrade_cross_module_refs() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/object_cross_module_ref").await;
    let package_v1 = runner.package.0;

    // create instances of objects within module and cross module
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v1)::base::make_objs() };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    assert_eq!(effects.created().len(), 2);

    // Upgrade and cross module, cross version type usage
    let (digest, modules) = build_upgrade_test_modules("object_cross_module_ref1");
    let effects = runner
        .upgrade(
            UpgradePolicy::COMPATIBLE,
            digest,
            modules,
            vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
        )
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    let package_v2 = runner.package.0;

    // create instances of objects within module and cross module for v2
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v2)::base::make_objs() };
            move_call! { builder, (package_v2)::base::make_objs_v2() };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    assert_eq!(effects.created().len(), 5);

    // Upgrade and cross module, cross version type usage
    let (digest, modules) = build_upgrade_test_modules("object_cross_module_ref2");
    let effects = runner
        .upgrade(
            UpgradePolicy::COMPATIBLE,
            digest,
            modules,
            vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
        )
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    let package_v2 = runner.package.0;

    // create instances of objects within module and cross module for v2
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v2)::base::make_objs() };
            move_call! { builder, (package_v2)::base::make_objs_v2() };
            move_call! { builder, (package_v2)::base::make_objs_v3() };
            builder.finish()
        })
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
    assert_eq!(effects.created().len(), 6);
}

#[tokio::test]
async fn test_upgrade_max_packages() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas_object_id)]).await;

    //
    // Build and publish max number of packages allowed
    let (_, modules, dependencies) = build_package("move_upgrade/base", false);

    // push max number of packages allowed to publish
    let max_pub_cmd = authority
        .epoch_store_for_testing()
        .protocol_config()
        .max_publish_or_upgrade_per_ptb_as_option()
        .unwrap_or(0);
    assert!(max_pub_cmd > 0);
    let packages = vec![(modules, dependencies); max_pub_cmd as usize];

    let mut builder = ProgrammableTransactionBuilder::new();
    build_multi_publish_txns(&mut builder, sender, packages);
    let result = run_multi_txns(&authority, sender, &sender_key, &gas_object_id, builder)
        .await
        .unwrap()
        .1;
    let effects = result.into_data();
    assert_eq!(effects.status(), &ExecutionStatus::Success);

    // collect package and upgrade caps
    let (digest, modules, dep_ids) = build_package("move_upgrade/base", false);
    let packages_and_upgrades = collect_packages_and_upgrade_caps(&authority, &effects).await;
    // (package id, upgrade cap ref, policy, digest, dep ids, modules)
    let mut package_upgrades: Vec<UpgradeData> = vec![];
    for (package_id, upgrade_cap) in packages_and_upgrades {
        package_upgrades.push(UpgradeData {
            package_id,
            upgrade_cap,
            policy: UpgradePolicy::COMPATIBLE,
            digest: digest.clone(),
            dep_ids: dep_ids.clone(),
            modules: modules.clone(),
        });
    }

    // Upgrade all packages
    let mut builder = ProgrammableTransactionBuilder::new();
    build_multi_upgrade_txns(&mut builder, package_upgrades);
    let result = run_multi_txns(&authority, sender, &sender_key, &gas_object_id, builder)
        .await
        .unwrap()
        .1;
    let effects = result.into_data();
    assert_eq!(effects.status(), &ExecutionStatus::Success);
}

#[tokio::test]
async fn test_upgrade_more_than_max_packages_error() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas_object_id)]).await;

    //
    // Build and publish max number of packages allowed
    let (_, modules, dependencies) = build_package("move_upgrade/base", false);

    // push max number of packages allowed to publish
    let max_pub_cmd = authority
        .epoch_store_for_testing()
        .protocol_config()
        .max_publish_or_upgrade_per_ptb_as_option()
        .unwrap_or(0);
    assert!(max_pub_cmd > 0);
    let packages = vec![(modules, dependencies); max_pub_cmd as usize];

    let mut builder = ProgrammableTransactionBuilder::new();
    build_multi_publish_txns(&mut builder, sender, packages);
    let result = run_multi_txns(&authority, sender, &sender_key, &gas_object_id, builder)
        .await
        .unwrap()
        .1;
    let effects = result.into_data();
    assert_eq!(effects.status(), &ExecutionStatus::Success);

    // collect package and upgrade caps
    let (digest, modules, dep_ids) = build_package("move_upgrade/base", false);
    let packages_and_upgrades = collect_packages_and_upgrade_caps(&authority, &effects).await;
    // (package id, upgrade cap ref, policy, digest, dep ids, modules)
    let mut package_upgrades: Vec<UpgradeData> = vec![];
    for (package_id, upgrade_cap) in packages_and_upgrades {
        package_upgrades.push(UpgradeData {
            package_id,
            upgrade_cap,
            policy: UpgradePolicy::COMPATIBLE,
            digest: digest.clone(),
            dep_ids: dep_ids.clone(),
            modules: modules.clone(),
        });
    }
    let (_, modules, dependencies) = build_package("object_basics", false);
    let packages = vec![(modules, dependencies); 2];

    // Upgrade all packages
    let mut builder = ProgrammableTransactionBuilder::new();
    build_multi_upgrade_txns(&mut builder, package_upgrades);
    build_multi_publish_txns(&mut builder, sender, packages);
    let err = run_multi_txns(&authority, sender, &sender_key, &gas_object_id, builder)
        .await
        .unwrap_err();
    assert_eq!(
        err,
        SuiError::UserInputError {
            error: UserInputError::MaxPublishCountExceeded {
                max_publish_commands: max_pub_cmd,
                publish_count: max_pub_cmd + 2,
            }
        }
    );
}

async fn assert_valid_dep_only_upgrade(runner: &mut UpgradeStateRunner, package_name: &str) {
    let (digest, modules) = build_upgrade_test_modules(package_name);
    let effects = runner
        .upgrade(
            UpgradePolicy::DEP_ONLY,
            digest,
            modules,
            vec![SUI_FRAMEWORK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID],
        )
        .await;

    assert!(effects.status().is_ok(), "{:#?}", effects.status());
}
