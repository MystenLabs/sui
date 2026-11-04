// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    adapter,
    data_store::{
        cached_package_store::CachedPackageStore,
        transaction_package_store::TransactionPackageStore,
    },
    execution_mode::{DevInspect, ExecutionMode, Genesis, Normal, System},
    gas_charger::GasCharger,
    static_programmable_transactions::{
        env::Env,
        linkage::{
            config::{NATIVE_PACKAGE_IDS, ResolutionConfig},
            resolution::{
                PackageResolution, ResolutionTable, VersionConstraint, add_and_unify, get_package,
            },
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
            single_linkage,
        },
        loading::{
            self,
            ast::{DeserializedPackage, PackagePayload},
        },
        metering::translation_meter::TranslationMeter,
        preparation::{self, ast as A},
        typing::{
            ast as T,
            invariant_checks::linkage_consistency::{self, verify_move_call, verify_type_linkage},
        },
    },
    temporary_store::TemporaryStore,
};
use move_binary_format::file_format::{Visibility, basic_test_module, empty_module};
use move_core_types::{
    gas_algebra::InternalGas,
    language_storage::{StructTag, TypeTag},
};
use move_vm_runtime::runtime::MoveRuntime;
use std::collections::BTreeMap;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    accumulator_root::EmptyUnsettledObjectFunds,
    base_types::{ObjectID, SuiAddress, SystemObjectVersions, TxContext},
    digests::TransactionDigest,
    error::{ExecutionError, UserInputError},
    execution_status::ExecutionErrorKind,
    in_memory_storage::InMemoryStorage,
    move_package::MovePackage,
    object::Object,
    transaction::{
        Command, GasData, InputObjects, ProgrammableMoveCall, ProgrammableTransaction,
        TransactionKind,
    },
};

const DEP: ObjectID = ObjectID::from_single_byte(10);
const DEP_V2: ObjectID = ObjectID::from_single_byte(11);
const USER: ObjectID = ObjectID::from_single_byte(12);

struct Packages {
    config: ProtocolConfig,
    runtime: MoveRuntime,
    storage: InMemoryStorage,
}

impl Packages {
    fn new() -> Self {
        let config = ProtocolConfig::get_for_max_version_UNSAFE();
        let mut module = basic_test_module();
        module.function_defs[0].visibility = Visibility::Public;
        module.address_identifiers = vec![DEP.into()];
        let dep = MovePackage::new_initial(&[module.clone()], &config, []).unwrap();
        let dep_v2 = dep.new_upgraded(DEP_V2, &[module], &config, []).unwrap();
        let mut user = empty_module();
        user.address_identifiers = vec![USER.into()];
        let user = MovePackage::new_initial(&[user], &config, [&dep]).unwrap();
        let storage = InMemoryStorage::new(
            vec![dep, dep_v2, user]
                .into_iter()
                .map(|pkg| Object::new_from_package(pkg, TransactionDigest::ZERO))
                .collect(),
        );
        let runtime = adapter::new_move_runtime(vec![], &config).unwrap();
        Self {
            config,
            runtime,
            storage,
        }
    }

    fn with_native_packages() -> Self {
        let mut p = Self::new();
        for id in NATIVE_PACKAGE_IDS {
            let mut module = if *id == sui_types::SUI_FRAMEWORK_PACKAGE_ID {
                basic_test_module()
            } else {
                empty_module()
            };
            module.address_identifiers = vec![(*id).into()];
            p.storage.insert_object(
                Object::new_package(&[module], TransactionDigest::ZERO, &p.config, []).unwrap(),
            );
        }
        p
    }

    fn store(&self) -> CachedPackageStore<'_, '_> {
        CachedPackageStore::new(&self.runtime, TransactionPackageStore::new(&self.storage))
    }

    fn resolution_config(&self) -> ResolutionConfig {
        ResolutionConfig::from_protocol_config::<Normal<ExecutionError>>(&self.config)
    }

    fn state(&self) -> TemporaryStore<'_> {
        TemporaryStore::new(
            &self.storage,
            InputObjects::new(vec![]),
            vec![],
            TransactionDigest::ZERO,
            &self.config,
            0,
            SystemObjectVersions::empty(),
            (
                &TransactionKind::ProgrammableTransaction(ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![],
                }),
                &GasData {
                    payment: vec![],
                    owner: SuiAddress::ZERO,
                    price: 1,
                    budget: 0,
                },
                SuiAddress::ZERO,
            ),
            &EmptyUnsettledObjectFunds,
        )
    }

    fn charger(&self, budget: u64) -> GasCharger {
        let mut charger = GasCharger::new_unmetered(TransactionDigest::ZERO, &self.config);
        // These tests inspect internal units directly, without computation bucketing.
        charger.move_gas_status_mut().set_metering(true);
        charger.move_gas_status_mut().gas_left = InternalGas::new(budget);
        charger
    }
}

fn call(type_arguments: Vec<TypeTag>) -> Command {
    Command::MoveCall(Box::new(ProgrammableMoveCall {
        package: DEP,
        module: "DUMMY".to_owned(),
        function: "foo".to_owned(),
        type_arguments: type_arguments.into_iter().map(Into::into).collect(),
        arguments: vec![],
    }))
}

fn old_type(address: ObjectID) -> TypeTag {
    TypeTag::Struct(Box::new(StructTag {
        address: address.into(),
        module: "DUMMY".parse().unwrap(),
        name: "Bar".parse().unwrap(),
        type_params: vec![],
    }))
}

// Expand USER repeatedly, then add DEP directly twice. The first expansion charges four
// linkage events, deduplicated expansions are free, and each direct contribution adds one charge.
#[test]
fn linkage_meter_charges_work_and_skips_deduplicated_expansion() {
    let p = Packages::new();
    let store = p.store();
    let mut charger = p.charger(1000);
    let rate = p.config.translation_per_linkage_entry_charge();
    let mut table = ResolutionTable::empty(p.resolution_config());
    {
        let mut meter = TranslationMeter::new(&p.config, &mut charger);
        let mut charge = || meter.charge_num_linkage_entries::<ExecutionError>(1);
        // USER's metadata lookup, contribution, self-linkage entry, and dependency are charged once.
        table
            .add_type_linkages_to_table([USER, USER], &store, &mut charge)
            .unwrap();
        table
            .add_type_linkages_to_table([USER], &store, &mut charge)
            .unwrap();
    }
    assert_eq!(
        u64::from(charger.move_gas_status().gas_left),
        1000 - 4 * rate
    );
    {
        let mut meter = TranslationMeter::new(&p.config, &mut charger);
        let mut charge = || meter.charge_num_linkage_entries::<ExecutionError>(1);
        // Repeated direct contributions still perform unification and are charged.
        for _ in 0..2 {
            add_and_unify(
                &DEP,
                &store,
                &mut table,
                VersionConstraint::at_least,
                &mut charge,
            )
            .unwrap();
        }
    }
    assert_eq!(
        u64::from(charger.move_gas_status().gas_left),
        1000 - 6 * rate
    );
    assert_eq!(table.resolution_table.len(), 2);
}

// Expand a package absent from storage. Its metadata lookup must consume one linkage charge
// before returning PublishUpgradeMissingDependency, without adding a resolution constraint.
#[test]
fn type_expansion_charges_before_loading_missing_package() {
    let p = Packages::new();
    let store = p.store();
    let mut charger = p.charger(100);
    let mut table = ResolutionTable::empty(p.resolution_config());
    {
        let mut meter = TranslationMeter::new(&p.config, &mut charger);
        let err = table
            .add_type_linkages_to_table([ObjectID::from_single_byte(255)], &store, &mut || {
                meter.charge_num_linkage_entries::<ExecutionError>(1)
            })
            .unwrap_err();
        assert_eq!(
            err.kind(),
            &ExecutionErrorKind::PublishUpgradeMissingDependency
        );
    }
    assert_eq!(
        u64::from(charger.move_gas_status().gas_left),
        100 - p.config.translation_per_linkage_entry_charge()
    );
    assert!(table.resolution_table.is_empty());
}

// Give normal-mode seeding exactly three linkage charges: all native packages must be seeded,
// another attempt must exhaust gas, and a framework call must load using that same linkage.
#[test]
fn native_package_seeding_is_metered() {
    let p = Packages::with_native_packages();
    let store = p.store();
    let analyzer = ResolutionConfig::from_protocol_config::<Normal<ExecutionError>>(&p.config);
    let rate = p.config.translation_per_linkage_entry_charge();
    let mut charger = p.charger(3 * rate);
    let table = {
        let mut meter = TranslationMeter::new(&p.config, &mut charger);
        let table = analyzer
            .resolution_table_with_native_packages(&store, &mut || {
                meter.charge_num_linkage_entries::<ExecutionError>(1)
            })
            .unwrap();
        assert_eq!(table.resolution_table.len(), 3);
        assert_eq!(
            table
                .resolution_table
                .get(&sui_types::SUI_FRAMEWORK_PACKAGE_ID)
                .unwrap()
                .object_id(),
            sui_types::SUI_FRAMEWORK_PACKAGE_ID
        );
        let err = analyzer
            .resolution_table_with_native_packages(&store, &mut || {
                meter.charge_num_linkage_entries::<ExecutionError>(1)
            })
            .unwrap_err();
        assert_eq!(err.kind(), &ExecutionErrorKind::InsufficientGas);
        table
    };
    assert_eq!(u64::from(charger.move_gas_status().gas_left), 0);
    // Normal linkage seeds the framework before loading, so synthesized calls need no
    // separate framework linkage or availability check.
    let linkage = ExecutableLinkage::new(ResolvedLinkage::from_resolution_table(table));
    let vm = p
        .runtime
        .make_vm(
            &store.package_store,
            linkage.linkage_context::<ExecutionError>().unwrap(),
        )
        .unwrap();
    let mut state = p.state();
    let env = Env::<Normal<ExecutionError>>::new(
        &p.config, &p.runtime, &mut state, &store, &analyzer, &vm, &linkage,
    );
    let function = env
        .load_framework_function(
            move_core_types::ident_str!("DUMMY"),
            move_core_types::ident_str!("foo"),
            vec![],
        )
        .unwrap();
    assert_eq!(
        *function.version_mid.address(),
        sui_types::SUI_FRAMEWORK_PACKAGE_ID.into()
    );
    assert!(std::rc::Rc::ptr_eq(&function.linkage.0, &linkage.0));
}

// Remove or remap each native package in otherwise consistent linkage. Normal and both
// dev-inspect modes must reject it; genesis/system modes must accept their native-free linkage.
#[test]
fn native_package_linkage_invariant_is_mode_dependent() {
    fn check<Mode: ExecutionMode<Error = ExecutionError>>(
        p: &Packages,
        corrupt: impl FnOnce(&mut ResolvedLinkage),
    ) -> Result<(), ExecutionError> {
        let store = p.store();
        let analyzer = ResolutionConfig::from_protocol_config::<Mode>(&p.config);
        let table = analyzer
            .resolution_table_with_native_packages(&store, &mut || Ok::<_, ExecutionError>(()))?;
        let mut linkage = ResolvedLinkage::from_resolution_table(table);
        // Construct the VM before corrupting linkage to exercise the invariant, not VM linking.
        let vm = p
            .runtime
            .make_vm(
                &store.package_store,
                ExecutableLinkage::new(ResolvedLinkage {
                    linkage: linkage.linkage.clone(),
                    linkage_resolution: linkage.linkage_resolution.clone(),
                })
                .linkage_context::<ExecutionError>()?,
            )
            .unwrap();
        corrupt(&mut linkage);
        let linkage = ExecutableLinkage::new(linkage);
        // Both corruptions remain structurally consistent; the native-package rule must catch them.
        linkage.validate_runtime_linkage_consistency::<ExecutionError>()?;
        let mut state = p.state();
        let env = Env::<Mode>::new(
            &p.config, &p.runtime, &mut state, &store, &analyzer, &vm, &linkage,
        );
        let txn = T::Transaction {
            gas_payment: None,
            bytes: Default::default(),
            objects: vec![],
            withdrawals: vec![],
            pure: vec![],
            receiving: vec![],
            withdrawal_compatibility_conversions: Default::default(),
            original_command_len: 0,
            commands: vec![],
            unified_linkage: linkage.clone(),
        };
        linkage_consistency::verify(&env, &txn)
    }

    let p = Packages::with_native_packages();
    check::<Normal<ExecutionError>>(&p, |_| {}).unwrap();
    check::<DevInspect<false>>(&p, |_| {}).unwrap();
    check::<DevInspect<true>>(&p, |_| {}).unwrap();
    // Predefined-package modes have empty linkage here and must not require native packages.
    check::<Genesis>(&p, |_| {}).unwrap();
    check::<System<ExecutionError>>(&p, |_| {}).unwrap();
    for id in NATIVE_PACKAGE_IDS {
        for replacement in [None, Some(ObjectID::from_single_byte(255))] {
            let corrupt = |linkage: &mut ResolvedLinkage| {
                if let Some(replacement) = replacement {
                    linkage.linkage.insert(*id, replacement);
                    linkage.linkage_resolution.insert(
                        replacement,
                        PackageResolution {
                            original_id: *id,
                            version: Some(2),
                        },
                    );
                } else {
                    linkage.linkage.remove(id);
                }
            };
            assert_invariant_failure(std::panic::AssertUnwindSafe(|| {
                check::<Normal<ExecutionError>>(&p, corrupt)
            }));
            assert_invariant_failure(std::panic::AssertUnwindSafe(|| {
                check::<DevInspect<false>>(&p, corrupt)
            }));
            assert_invariant_failure(std::panic::AssertUnwindSafe(|| {
                check::<DevInspect<true>>(&p, corrupt)
            }));
        }
    }
}

// Add conflicting exact versions with a two-charge budget, then retry after exhaustion.
// The conflict must be charged; the retry must return InsufficientGas before changing the table.
#[test]
fn linkage_meter_charges_conflicts_and_exhausts_before_processing() {
    let p = Packages::new();
    let store = p.store();
    let rate = p.config.translation_per_linkage_entry_charge();
    let mut charger = p.charger(2 * rate);
    let mut table = ResolutionTable::empty(p.resolution_config());
    {
        let mut meter = TranslationMeter::new(&p.config, &mut charger);
        let mut charge = || meter.charge_num_linkage_entries::<ExecutionError>(1);
        add_and_unify(
            &DEP,
            &store,
            &mut table,
            VersionConstraint::exact,
            &mut charge,
        )
        .unwrap();
        let err = add_and_unify(
            &DEP_V2,
            &store,
            &mut table,
            VersionConstraint::exact,
            &mut charge,
        )
        .unwrap_err();
        assert_eq!(err.kind(), &ExecutionErrorKind::InvalidLinkage);
        let err = add_and_unify(
            &DEP_V2,
            &store,
            &mut table,
            VersionConstraint::exact,
            &mut charge,
        )
        .unwrap_err();
        assert_eq!(err.kind(), &ExecutionErrorKind::InsufficientGas);
    }
    assert_eq!(u64::from(charger.move_gas_status().gas_left), 0);
    assert_eq!(table.resolution_table.get(&DEP).unwrap().object_id(), DEP);
    assert!(!table.all_versions_resolution_table.contains_key(&DEP_V2));
}

// Name a v1-defined type through v2 while giving nongeneric foo a type argument. Normal-mode
// preparation must normalize to v1 without selecting v2; loading must reject the invalid arity.
#[test]
fn prepared_ast_normalizes_names_but_defers_instantiation_validation() {
    let p = Packages::with_native_packages();
    let store = p.store();
    let analyzer = p.resolution_config();
    let mut state = p.state();
    let mut charger = p.charger(10000);
    let mut meter = TranslationMeter::new(&p.config, &mut charger);
    let prepared = preparation::translate::transaction::<Normal<ExecutionError>>(
        &mut meter,
        &analyzer,
        &store,
        &state,
        &p.config,
        ProgrammableTransaction {
            inputs: vec![],
            commands: vec![call(vec![old_type(DEP_V2)])],
        },
    )
    .unwrap();
    let A::Command::MoveCall(call) = &prepared.commands[0] else {
        panic!("expected call")
    };
    assert_eq!(call.type_arguments, vec![old_type(DEP)]);
    assert_eq!(prepared.unified_linkage.0.linkage.get(&DEP), Some(&DEP));
    assert!(
        !prepared
            .unified_linkage
            .0
            .linkage_resolution
            .contains_key(&DEP_V2)
    );
    let linkage = prepared.unified_linkage.clone();
    let vm = p
        .runtime
        .make_vm(
            &store.package_store,
            linkage.linkage_context::<ExecutionError>().unwrap(),
        )
        .unwrap();
    let env = Env::<Normal<ExecutionError>>::new(
        &p.config, &p.runtime, &mut state, &store, &analyzer, &vm, &linkage,
    );
    let tx_context = TxContext::new_from_components(
        &sui_types::base_types::SuiAddress::ZERO,
        &TransactionDigest::ZERO,
        &0,
        0,
        1,
        1,
        10000,
        None,
        &p.config,
    );
    // foo has no type parameters. Preparation intentionally succeeds; VM loading must reject it.
    assert!(
        loading::translate::transaction::<Normal<ExecutionError>>(
            &mut meter,
            &env,
            &tx_context,
            None,
            None,
            prepared
        )
        .is_err()
    );
}

// Prepare an out-of-range input reference in normal mode with native packages absent.
// Argument validation must fail before native seeding or any linkage gas is consumed.
#[test]
fn preparation_validates_argument_indices_before_linkage_work() {
    let p = Packages::new();
    let store = p.store();
    let analyzer = p.resolution_config();
    let state = p.state();
    let mut charger = p.charger(10000);
    let mut meter = TranslationMeter::new(&p.config, &mut charger);
    assert_invariant_failure(std::panic::AssertUnwindSafe(|| {
        preparation::translate::transaction::<Normal<ExecutionError>>(
            &mut meter,
            &analyzer,
            &store,
            &state,
            &p.config,
            ProgrammableTransaction {
                inputs: vec![],
                commands: vec![Command::MakeMoveVec(
                    None,
                    vec![sui_types::transaction::Argument::Input(0)],
                )],
            },
        )
        .map(|_| ())
    }));
    let base_charges = p.config.translation_per_input_base_charge()
        + 2 * p.config.translation_per_command_base_charge();
    assert_eq!(
        u64::from(charger.move_gas_status().gas_left),
        10000 - base_charges
    );
}

// Check vector/struct chains in MoveCall and MakeMoveVec at admission depth/node boundaries.
// Admitted chains must normalize every address in normal mode; boundary chains must be rejected.
#[test]
fn transaction_type_limits_bound_recursive_normalization() {
    let p = Packages::with_native_packages();
    let store = p.store();
    let state = p.state();
    for depth_limit in [8, p.config.max_type_argument_depth()] {
        let mut config = p.config.clone();
        config.set_max_type_argument_depth_for_testing(depth_limit);
        let analyzer = ResolutionConfig::from_protocol_config::<Normal<ExecutionError>>(&config);
        for structs in [false, true] {
            let nested_type = |depth, address: ObjectID| {
                let mut ty = TypeTag::Bool;
                for _ in 1..depth {
                    ty = if structs {
                        TypeTag::Struct(Box::new(StructTag {
                            address: address.into(),
                            module: "DUMMY".parse().unwrap(),
                            name: "Bar".parse().unwrap(),
                            type_params: vec![ty],
                        }))
                    } else {
                        TypeTag::Vector(Box::new(ty))
                    };
                }
                ty
            };
            // The validity check counts the root at depth one and uses a strict upper bound.
            for depth in [depth_limit - 1, depth_limit] {
                let ty = nested_type(depth, DEP_V2);
                for command in [
                    call(vec![ty.clone()]),
                    Command::MakeMoveVec(Some(ty.clone().into()), vec![]),
                ] {
                    let txn = ProgrammableTransaction {
                        inputs: vec![],
                        commands: vec![command],
                    };
                    let checked = TransactionKind::ProgrammableTransaction(txn.clone())
                        .validity_check(&config);
                    if depth == depth_limit {
                        let UserInputError::SizeLimitExceeded { limit, .. } = checked.unwrap_err()
                        else {
                            panic!("expected a type size/depth limit error")
                        };
                        if depth_limit == 8 {
                            assert!(limit.contains("depth"));
                        }
                        continue;
                    }
                    checked.unwrap();
                    let mut charger = p.charger(10000);
                    let mut meter = TranslationMeter::new(&config, &mut charger);
                    let prepared = preparation::translate::transaction::<Normal<ExecutionError>>(
                        &mut meter, &analyzer, &store, &state, &config, txn,
                    )
                    .unwrap();
                    let normalized = match &prepared.commands[0] {
                        A::Command::MoveCall(call) => &call.type_arguments[0],
                        A::Command::MakeMoveVec(Some(ty), _) => ty,
                        _ => panic!("expected a command with an explicit type"),
                    };
                    assert_eq!(normalized, &nested_type(depth, DEP));
                }
            }
        }
    }
}

// Prepare a no-init publish against v2 alongside a v1 call and a normalized vector type.
// Payload/order/publication linkage must be retained, runtime must stay on v1, and normal-mode
// gas must include native seeding for both contexts plus the six other linkage events.
#[test]
fn prepared_ast_retains_publications_without_contributing_to_runtime() {
    let p = Packages::with_native_packages();
    let store = p.store();
    let analyzer = p.resolution_config();
    let state = p.state();
    let mut charger = p.charger(10000);
    let mut meter = TranslationMeter::new(&p.config, &mut charger);
    let mut bytes = vec![];
    empty_module()
        .serialize_with_version(p.config.move_binary_format_version(), &mut bytes)
        .unwrap();
    let prepared = preparation::translate::transaction::<Normal<ExecutionError>>(
        &mut meter,
        &analyzer,
        &store,
        &state,
        &p.config,
        ProgrammableTransaction {
            inputs: vec![],
            commands: vec![
                Command::Publish(vec![bytes.clone()], vec![DEP_V2]),
                call(vec![]),
                Command::MakeMoveVec(Some(old_type(DEP_V2).into()), vec![]),
            ],
        },
    )
    .unwrap();
    let A::Command::Publish(PackagePayload::Deserialized(pkg), deps, linkage) =
        &prepared.commands[0]
    else {
        panic!("expected publication")
    };
    assert_eq!(deps, &vec![DEP_V2]);
    assert_eq!(pkg.total_bytes, bytes.len());
    assert_eq!(
        pkg.computed_digest,
        MovePackage::compute_digest_for_modules_and_deps(&[bytes], deps, true)
    );
    assert_eq!(pkg.deserialized_modules.len(), 1);
    assert_eq!(linkage.linkage.get(&DEP), Some(&DEP_V2));
    assert_eq!(prepared.unified_linkage.0.linkage.get(&DEP), Some(&DEP));
    assert!(matches!(&prepared.commands[1], A::Command::MoveCall(_)));
    let A::Command::MakeMoveVec(Some(ty), _) = &prepared.commands[2] else {
        panic!("expected vector")
    };
    assert_eq!(ty, &old_type(DEP));
    // Native packages are seeded once for runtime and once for the publication context.
    // Publication adds one dependency, the call adds two entries (target and self), and vector
    // expansion charges its metadata lookup plus those two entries. Base charges cover the empty
    // input list and the commands' argument counts (2, 1, 1).
    let base_charges = p.config.translation_per_input_base_charge()
        + 4 * p.config.translation_per_command_base_charge();
    let expected = base_charges
        + (2 * NATIVE_PACKAGE_IDS.len() as u64 + 6)
            * p.config.translation_per_linkage_entry_charge();
    assert_eq!(
        u64::from(charger.move_gas_status().gas_left),
        10000 - expected
    );
}

// Analyze a new-module initializer with the upgraded package absent, present and conflicting,
// or present and agreeing. Assert each branch's charges and exact pin, plus gas-exhaustion precedence.
#[test]
fn upgrade_membership_and_equality_checks_are_metered() {
    let p = Packages::new();
    let store = p.store();
    let mut module = basic_test_module();
    module.identifiers[0] = "NEW".parse().unwrap();
    module.identifiers[1] = "init".parse().unwrap();
    let payload = PackagePayload::Deserialized(DeserializedPackage::new(vec![module], 0, [0; 32]));
    let mut publication = ResolutionTable::empty(p.resolution_config());
    add_and_unify(
        &DEP_V2,
        &store,
        &mut publication,
        VersionConstraint::exact,
        &mut || Ok::<_, ExecutionError>(()),
    )
    .unwrap();
    let publication = ResolvedLinkage::from_resolution_table(publication);
    for (participates, selects_v2, budget) in [
        (false, false, 100),
        (true, false, 100),
        (true, true, 100),
        (true, false, 0),
    ] {
        let mut table = ResolutionTable::empty(p.resolution_config());
        let mut noop = || Ok::<_, ExecutionError>(());
        if participates {
            table
                .add_type_linkages_to_table::<_, ExecutionError, _>([USER], &store, &mut noop)
                .unwrap();
        } else {
            add_and_unify(
                &DEP,
                &store,
                &mut table,
                VersionConstraint::at_least,
                &mut noop,
            )
            .unwrap();
        }
        if selects_v2 {
            add_and_unify(
                &DEP_V2,
                &store,
                &mut table,
                VersionConstraint::at_least,
                &mut noop,
            )
            .unwrap();
        }
        let mut charger = p.charger(budget);
        let result = {
            let mut meter = TranslationMeter::new(&p.config, &mut charger);
            single_linkage::analyze_upgrade_command(
                &payload,
                &USER,
                &publication,
                &mut table,
                &store,
                &p.config,
                &mut || meter.charge_num_linkage_entries::<ExecutionError>(1),
            )
        };
        assert_eq!(
            u64::from(charger.move_gas_status().gas_left),
            budget.saturating_sub(
                (if participates && selects_v2 { 2 } else { 1 })
                    * p.config.translation_per_linkage_entry_charge()
            )
        );
        if budget == 0 {
            assert_eq!(
                result.unwrap_err().kind(),
                &ExecutionErrorKind::InsufficientGas
            );
        } else if participates && !selects_v2 {
            assert_eq!(
                result.unwrap_err().kind(),
                &ExecutionErrorKind::InvalidLinkage
            );
        } else {
            result.unwrap();
            assert_eq!(
                table.resolution_table.get(&DEP).unwrap().object_id(),
                DEP_V2
            );
        }
    }
}

// Prepare and load a normal-mode call, then replace its linkage with an equal allocation or
// change its target version. The valid call must share the canonical Rc; both corruptions must fail.
#[test]
fn loaded_calls_share_linkage_and_invariants_reject_equal_copies_and_wrong_targets() {
    let p = Packages::with_native_packages();
    let store = p.store();
    let analyzer = p.resolution_config();
    let mut state = p.state();
    let mut charger = p.charger(10000);
    let mut meter = TranslationMeter::new(&p.config, &mut charger);
    let prepared = preparation::translate::transaction::<Normal<ExecutionError>>(
        &mut meter,
        &analyzer,
        &store,
        &state,
        &p.config,
        ProgrammableTransaction {
            inputs: vec![],
            commands: vec![call(vec![])],
        },
    )
    .unwrap();
    let linkage = prepared.unified_linkage.clone();
    let vm = p
        .runtime
        .make_vm(
            &store.package_store,
            linkage.linkage_context::<ExecutionError>().unwrap(),
        )
        .unwrap();
    let env = Env::<Normal<ExecutionError>>::new(
        &p.config, &p.runtime, &mut state, &store, &analyzer, &vm, &linkage,
    );
    let tx_context = TxContext::new_from_components(
        &sui_types::base_types::SuiAddress::ZERO,
        &TransactionDigest::ZERO,
        &0,
        0,
        1,
        1,
        10000,
        None,
        &p.config,
    );
    let loaded = loading::translate::transaction::<Normal<ExecutionError>>(
        &mut meter,
        &env,
        &tx_context,
        None,
        None,
        prepared,
    )
    .unwrap();
    assert!(std::rc::Rc::ptr_eq(
        &loaded.unified_linkage.0,
        &env.unified_linkage.0
    ));
    let mut commands = loaded.commands.into_iter();
    let loading::ast::Command::MoveCall(mut call) = commands.next().unwrap() else {
        panic!("expected call")
    };
    verify_move_call::<Normal<ExecutionError>>(&env, &call.function).unwrap();
    call.function.linkage = ExecutableLinkage::new(ResolvedLinkage {
        linkage: linkage.0.linkage.clone(),
        linkage_resolution: linkage.0.linkage_resolution.clone(),
    });
    assert_invariant_failure(std::panic::AssertUnwindSafe(|| {
        verify_move_call::<Normal<ExecutionError>>(&env, &call.function)
    }));
    call.function.linkage = linkage.clone();
    call.function.version_mid =
        move_core_types::language_storage::ModuleId::new(DEP_V2.into(), "DUMMY".parse().unwrap());
    assert_invariant_failure(std::panic::AssertUnwindSafe(|| {
        verify_move_call::<Normal<ExecutionError>>(&env, &call.function)
    }));
}

fn assert_invariant_failure(
    f: impl FnOnce() -> Result<(), ExecutionError> + std::panic::UnwindSafe,
) {
    let result = std::panic::catch_unwind(f);
    if cfg!(debug_assertions) {
        assert!(result.is_err());
    } else {
        assert!(result.unwrap().is_err());
    }
}

// Corrupt resolution metadata by removing an entry, changing its original ID, or late-binding
// its version. Each must fail runtime validation; a known type with no linkage must also fail.
#[test]
fn runtime_invariants_reject_missing_mismatched_or_late_bound_versions() {
    let p = Packages::new();
    let store = p.store();
    for corruption in 0..3 {
        let mut table = ResolutionTable::empty(p.resolution_config());
        add_and_unify(
            &DEP,
            &store,
            &mut table,
            VersionConstraint::exact,
            &mut || Ok::<_, ExecutionError>(()),
        )
        .unwrap();
        let mut linkage = ResolvedLinkage::from_resolution_table(table);
        match corruption {
            0 => {
                linkage.linkage_resolution.remove(&DEP);
            }
            1 => {
                linkage
                    .linkage_resolution
                    .get_mut(&DEP)
                    .unwrap()
                    .original_id = USER;
            }
            _ => {
                linkage.linkage_resolution.get_mut(&DEP).unwrap().version = None;
            }
        }
        let linkage = ExecutableLinkage::new(linkage);
        assert_invariant_failure(|| {
            linkage.validate_runtime_linkage_consistency::<ExecutionError>()
        });
    }
    let empty = ExecutableLinkage::new(ResolvedLinkage {
        linkage: BTreeMap::new(),
        linkage_resolution: BTreeMap::new(),
    });
    let vm_type =
        crate::static_programmable_transactions::loading::ast::Type::Datatype(std::rc::Rc::new(
            crate::static_programmable_transactions::loading::ast::Datatype {
                abilities: move_binary_format::file_format::AbilitySet::EMPTY,
                module: move_core_types::language_storage::ModuleId::new(
                    DEP.into(),
                    "DUMMY".parse().unwrap(),
                ),
                name: "Bar".parse().unwrap(),
                type_arguments: vec![],
            },
        ));
    assert_invariant_failure(|| verify_type_linkage::<Normal<ExecutionError>>([&vm_type], &empty));
    // A real package is still available: missing linkage metadata must not be silently repaired.
    assert!(get_package::<ExecutionError, _>(&DEP, &store).is_ok());
}
