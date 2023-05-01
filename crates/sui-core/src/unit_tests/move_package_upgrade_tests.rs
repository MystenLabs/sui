// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{ident_str, language_storage::StructTag};
use sui_move_build::BuildConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    messages::{Argument, ObjectArg, ProgrammableTransaction, TEST_ONLY_GAS_UNIT_FOR_PUBLISH},
    move_package::UpgradePolicy,
    object::{Object, Owner},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::{BackingPackageStore, ObjectStore},
    MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID,
};

use std::{collections::BTreeSet, path::PathBuf, str::FromStr, sync::Arc};
use sui_types::effects::{TransactionEffects, TransactionEffectsV1};
use sui_types::execution_status::{
    CommandArgumentError, ExecutionFailureStatus, PackageUpgradeError,
};

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::{
    authority_test_utils::build_test_modules_with_dep_addr,
    authority_tests::execute_programmable_transaction,
    move_integration_tests::build_and_publish_test_package_with_upgrade_cap, AuthorityState,
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

fn build_upgrade_test_modules(test_dir: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "move_upgrade", test_dir]);
    let with_unpublished_deps = false;
    let hash_modules = true;
    let package = BuildConfig::new_for_testing().build(path).unwrap();
    (
        package
            .get_package_digest(with_unpublished_deps, hash_modules)
            .to_vec(),
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
    let package = build_test_modules_with_dep_addr(path, dep_original_addresses, dep_ids);
    let with_unpublished_deps = false;
    let hash_modules = true;
    (
        package
            .get_package_digest(with_unpublished_deps, hash_modules)
            .to_vec(),
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
        (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(Argument::Input(0), upgrade_arg, digest_arg)
    };
    let upgrade_receipt = builder.upgrade(current_pkg_id, upgrade_ticket, vec![], modules);
    move_call! {
        builder,
        (SUI_FRAMEWORK_OBJECT_ID)::package::commit_upgrade(Argument::Input(0), upgrade_receipt)
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
}

impl UpgradeStateRunner {
    pub async fn new(base_package_name: &str) -> Self {
        let _dont_remove = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_package_upgrades_for_testing(true);
            config
        });
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas_object_id = ObjectID::random();
        let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
        let authority_state = TestAuthorityBuilder::new().build().await;
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
        let TransactionEffects::V1(effects) = self.run(pt).await;
        assert!(effects.status.is_ok(), "{:#?}", effects.status);

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

    pub async fn upgrade(
        &mut self,
        policy: u8,
        digest: Vec<u8>,
        modules: Vec<Vec<u8>>,
        dep_ids: Vec<ObjectID>,
    ) -> TransactionEffectsV1 {
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
                (SUI_FRAMEWORK_OBJECT_ID)::package::authorize_upgrade(cap, policy, digest)
            };

            let receipt = builder.upgrade(package_id, ticket, dep_ids, modules);
            move_call! { builder, (SUI_FRAMEWORK_OBJECT_ID)::package::commit_upgrade(cap, receipt) };

            builder.finish()
        };

        let TransactionEffects::V1(effects) = self.run(pt).await;
        if effects.status.is_ok() {
            self.package = *effects
                .created
                .iter()
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
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        )
        .await
        .unwrap();

        let TransactionEffects::V1(fx) = &effects;

        if let Some(updated_cap) = fx
            .mutated
            .iter()
            .find_map(|(cap, _)| (cap.0 == self.upgrade_cap.0).then_some(cap))
        {
            self.upgrade_cap = *updated_cap;
        }

        effects
    }
}

#[tokio::test]
async fn test_upgrade_package_happy_path() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let TransactionEffects::V1(effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (runner.package.0)::base::return_0()
            };

            builder.finish()
        })
        .await;

    match effects.status.unwrap_err().0 {
        ExecutionFailureStatus::MoveAbort(_, 42) => { /* nop */ }
        err => panic!("Unexpected error: {:#?}", err),
    };

    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    let package = runner
        .authority_state
        .database
        .get_package(&runner.package.0)
        .unwrap()
        .unwrap();
    let config = ProtocolConfig::get_for_max_version();
    let normalized_modules = package
        .normalize(
            config.move_binary_format_version(),
            config.no_extraneous_module_bytes(),
        )
        .unwrap();
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
    let TransactionEffects::V1(effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (runner.package.0)::base::return_0()
            };

            builder.finish()
        })
        .await;
    assert!(effects.status.is_ok(), "{:#?}", effects.status);
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
            vec![SUI_FRAMEWORK_OBJECT_ID, MOVE_STDLIB_OBJECT_ID],
        )
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
    let package_v2 = runner.package.0;

    // Second upgrade introduces an entry function that creates `B`s.
    let (digest, modules) = build_upgrade_test_modules("makes_new_object");
    let effects = runner
        .upgrade(
            UpgradePolicy::COMPATIBLE,
            digest,
            modules,
            vec![SUI_FRAMEWORK_OBJECT_ID, MOVE_STDLIB_OBJECT_ID],
        )
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
    let package_v3 = runner.package.0;

    // Create an instance of the type introduced at version 2, with the function introduced at
    // version 3.
    let TransactionEffects::V1(effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v3)::base::makes_b() };
            builder.finish()
        })
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
    let created = effects
        .created
        .iter()
        .find_map(|(b, owner)| matches!(owner, Owner::AddressOwner(_)).then_some(b))
        .unwrap();

    let b = runner
        .authority_state
        .database
        .get_object_by_key(&created.0, created.1)
        .unwrap()
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
    let TransactionEffects::V1(effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            let b = builder.obj(ObjectArg::ImmOrOwnedObject(*created)).unwrap();
            move_call! { builder, (package_v3)::base::destroys_b(b) };
            builder.finish()
        })
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
}

#[tokio::test]
async fn test_upgrade_incompatible() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("compatibility_invalid");
    let effects = runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    assert_eq!(
        effects.status.unwrap_err().0,
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
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::DigestDoesNotMatch { digest }
        }
    );
}

#[tokio::test]
async fn test_upgrade_package_compatibility_too_permissive() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let TransactionEffects::V1(effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            let cap = builder
                .obj(ObjectArg::ImmOrOwnedObject(runner.upgrade_cap))
                .unwrap();
            move_call! { builder, (SUI_FRAMEWORK_OBJECT_ID)::package::only_dep_upgrades(cap) };
            builder.finish()
        })
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);

    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    let effects = runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    // ETooPermissive abort when we try to authorize the upgrade.
    assert!(matches!(
        effects.status.unwrap_err().0,
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
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::IncompatibleUpgrade
        },
    );
}

#[tokio::test]
async fn test_upgrade_package_compatible_in_additive_mode() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    let effects = runner
        .upgrade(UpgradePolicy::ADDITIVE, digest, modules, vec![])
        .await;

    assert_eq!(
        effects.status.unwrap_err().0,
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
        effects.status.unwrap_err().0,
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
        effects.status.unwrap_err().0,
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
        effects.status.unwrap_err().0,
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

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
}

#[tokio::test]
async fn test_upgrade_package_invalid_additive_mode() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("additive_upgrade_invalid");
    let effects = runner
        .upgrade(UpgradePolicy::ADDITIVE, digest, modules, vec![])
        .await;

    assert_eq!(
        effects.status.unwrap_err().0,
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
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::IncompatibleUpgrade
        },
    );
}

#[tokio::test]
async fn test_upgrade_package_dep_only_mode() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;

    let (digest, modules) = build_upgrade_test_modules("dep_only_upgrade");
    let effects = runner
        .upgrade(
            UpgradePolicy::DEP_ONLY,
            digest,
            modules,
            vec![SUI_FRAMEWORK_OBJECT_ID, MOVE_STDLIB_OBJECT_ID],
        )
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
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
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let (_, effects) = test_multiple_upgrades(&mut runner, true).await;
    assert!(matches!(
        effects.status.unwrap_err().0,
        ExecutionFailureStatus::PackageUpgradeError {
            upgrade_error: PackageUpgradeError::DigestDoesNotMatch { digest: _ }
        }
    ));
}

#[tokio::test]
async fn test_multiple_upgrades_valid() {
    let mut runner = UpgradeStateRunner::new("move_upgrade/base").await;
    let (_, effects) = test_multiple_upgrades(&mut runner, false).await;
    assert!(effects.status.is_ok(), "{:#?}", effects.status);
}

async fn test_multiple_upgrades(
    runner: &mut UpgradeStateRunner,
    use_empty_deps: bool,
) -> (ObjectID, TransactionEffectsV1) {
    let (digest, modules) = build_upgrade_test_modules("stage1_basic_compatibility_valid");
    let effects = runner
        .upgrade(UpgradePolicy::COMPATIBLE, digest, modules, vec![])
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);

    let package_v2 = effects
        .created
        .iter()
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
                vec![SUI_FRAMEWORK_OBJECT_ID, MOVE_STDLIB_OBJECT_ID]
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
    assert!(effects.status.is_ok(), "{:#?}", effects.status);

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
    assert!(effects.status.is_ok(), "{:#?}", effects.status);
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

    let TransactionEffects::V1(effects) = runner.run(pt1).await;
    assert!(effects.status.is_ok(), "{:#?}", effects.status);

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
        [
            ("package_upgrade_base", dep_v2_package.0),
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
        .database
        .get_package(&root_package.0)
        .unwrap()
        .unwrap();

    // Make sure the linkage table points to the correct versions!
    let dep_ids_in_linkage_table: BTreeSet<_> = root_move_package
        .linkage_table()
        .values()
        .map(|up| up.upgraded_id)
        .collect();
    assert!(dep_ids_in_linkage_table.contains(&runner.package.0));
    assert!(dep_ids_in_linkage_table.contains(&depender_package.0));

    // Call into the root module to call base module's function (should abort due to base module's
    // call_return_0 aborting with code 42)
    let TransactionEffects::V1(call_effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (root_package.0)::my_module::call_return_0()
            };

            builder.finish()
        })
        .await;

    match call_effects.status.unwrap_err().0 {
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

    let TransactionEffects::V1(effects) = runner.run(pt1).await;
    assert!(effects.status.is_ok(), "{:#?}", effects.status);
    // Dependency graph: base(v1) <-- dep_on_upgrading_package
    //                   base(v2)

    let base_v2_package = effects
        .created
        .iter()
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
        .database
        .get_package(&root_package.0)
        .unwrap()
        .unwrap();

    // Make sure the linkage table points to the correct versions!
    let dep_ids_in_linkage_table: BTreeSet<_> = root_move_package
        .linkage_table()
        .values()
        .map(|up| up.upgraded_id)
        .collect();
    assert!(dep_ids_in_linkage_table.contains(&base_v2_package.0));
    assert!(dep_ids_in_linkage_table.contains(&depender_package.0));

    // Call into the root module to call upgraded base module's function (should succeed due to base module's
    // call_return_0 no longer aborting)
    let TransactionEffects::V1(call_effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (root_package.0)::my_module::call_return_0()
            };

            builder.finish()
        })
        .await;

    assert!(call_effects.status.is_ok(), "{:#?}", call_effects.status);
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
            vec![SUI_FRAMEWORK_OBJECT_ID, MOVE_STDLIB_OBJECT_ID],
        )
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
    let package_v2 = runner.package.0;

    // Second upgrade (version 3) introduces a new type, C.
    let (digest, modules) = build_upgrade_test_modules("makes_another_object");
    let effects = runner
        .upgrade(
            UpgradePolicy::COMPATIBLE,
            digest,
            modules,
            vec![SUI_FRAMEWORK_OBJECT_ID, MOVE_STDLIB_OBJECT_ID],
        )
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
    let package_v3 = runner.package.0;

    // Create an instance of the type introduced at version 2 using function from version 2.
    let TransactionEffects::V1(effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v2)::base::makes_b() };
            builder.finish()
        })
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
    let created_b = effects
        .created
        .iter()
        .find_map(|(b, owner)| matches!(owner, Owner::AddressOwner(_)).then_some(b))
        .unwrap();

    // Create an instance of the type introduced at version 3 using function from version 3.
    let TransactionEffects::V1(effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v3)::base::makes_c() };
            builder.finish()
        })
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
    let created_c = effects
        .created
        .iter()
        .find_map(|(c, owner)| matches!(owner, Owner::AddressOwner(_)).then_some(c))
        .unwrap();

    // modify objects created of types introduced at versions 2 and 3 and emit events using types
    // introduced at versions 2 and 3 (using functions from version 3)
    let TransactionEffects::V1(effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            let b = builder
                .obj(ObjectArg::ImmOrOwnedObject(*created_b))
                .unwrap();
            move_call! { builder, (package_v3)::base::modifies_b(b) };
            let c = builder
                .obj(ObjectArg::ImmOrOwnedObject(*created_c))
                .unwrap();
            move_call! { builder, (package_v3)::base::modifies_c(c) };
            builder.finish()
        })
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);

    // verify that the types of events match
    let e1_type = StructTag::from_str(&format!("{package_v2}::base::BModEvent")).unwrap();
    let e2_type = StructTag::from_str(&format!("{package_v3}::base::CModEvent")).unwrap();

    let event_digest = effects.events_digest.unwrap();
    let mut events = runner
        .authority_state
        .get_transaction_events(&event_digest)
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
    assert!(effects.status.is_ok(), "{:#?}", effects.status);

    let package_v3 = effects
        .created
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0
         .0;

    // call the same function twice within the same block but from two different module versions
    let TransactionEffects::V1(effects) = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! { builder, (package_v2)::base::return_0() };
            move_call! { builder, (package_v3)::base::return_0() };
            builder.finish()
        })
        .await;

    assert!(effects.status.is_ok(), "{:#?}", effects.status);
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

    let TransactionEffects::V1(effects) = runner.run(pt1).await;
    assert!(effects.status.is_ok(), "{:#?}", effects.status);

    let base_v2_package = effects
        .created
        .iter()
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
    assert!(effects.status.is_ok(), "{:#?}", effects.status);

    let dependent_v2_package = effects
        .created
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap()
        .0;

    // call the same function twice within the same block but from two different module versions
    // that differ only by having different dependencies
    let TransactionEffects::V1(effects) = runner
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

    let call_error = effects.status.unwrap_err();

    // verify that execution aborts
    match call_error.0 {
        ExecutionFailureStatus::MoveAbort(_, 42) => { /* nop */ }
        err => panic!("Unexpected error: {:#?}", err),
    };

    // verify that execution aborts in the second (counting from 0) command
    assert_eq!(call_error.1, Some(1));
}
