// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Simplifies logic around re-using ModuleIds.
#![allow(clippy::redundant_clone)]

use crate::{
    cache::identifier_interner::intern_ident_str,
    dev_utils::{
        compilation_utils::{compile_packages_in_file, expect_modules},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::{InMemoryStorage, StoredPackage},
        vm_test_adapter::VMTestAdapter,
    },
    execution::dispatch_tables::{DepthFormula, IntraPackageKey, VirtualTableKey},
    jit::execution::ast::Type,
    natives::functions::NativeFunctions,
    runtime::MoveRuntime,
    shared::{
        gas::UnmeteredGasMeter,
        linkage_context::LinkageContext,
        serialization::SerializedReturnValues,
        types::{OriginalId, VersionId},
    },
};
use move_binary_format::{
    errors::VMResult,
    file_format::{
        empty_module, AddressIdentifierIndex, IdentifierIndex, ModuleHandle, TableIndex,
    },
    CompiledModule,
};
use move_compiler::Compiler;
use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
    resolver::TypeOrigin,
    runtime_value::MoveValue,
};
use move_vm_config::{runtime::VMConfig, verifier::VerifierConfig};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::{collections::BTreeMap, path::PathBuf, str::FromStr, sync::Arc, thread};

const ADDR2: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 2u8;
    AccountAddress::new(address)
};
const ADDR3: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 3u8;
    AccountAddress::new(address)
};
const ADDR4: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 4u8;
    AccountAddress::new(address)
};
const ADDR5: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 5u8;
    AccountAddress::new(address)
};
const ADDR6: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 6u8;
    AccountAddress::new(address)
};

static ADDR2_FUNCTIONS: Lazy<Vec<(ModuleId, Identifier)>> = Lazy::new(|| {
    vec![
        (
            ModuleId::new(ADDR2, Identifier::new("A").unwrap()),
            Identifier::new("entry_a").unwrap(),
        ),
        (
            ModuleId::new(ADDR2, Identifier::new("D").unwrap()),
            Identifier::new("entry_d").unwrap(),
        ),
        (
            ModuleId::new(ADDR2, Identifier::new("E").unwrap()),
            Identifier::new("entry_e").unwrap(),
        ),
        (
            ModuleId::new(ADDR2, Identifier::new("F").unwrap()),
            Identifier::new("entry_f").unwrap(),
        ),
        (
            ModuleId::new(ADDR2, Identifier::new("C").unwrap()),
            Identifier::new("just_c").unwrap(),
        ),
    ]
});

struct Adapter {
    runtime_adapter: Arc<RwLock<InMemoryTestAdapter>>,
    store: RelinkingStore,
}

#[derive(Clone)]
struct RelinkingStore {
    linkage: LinkageContext,
    // TODO: when we add type origin to `LinkageContext`, we should remove this field
    type_origin: Vec<TypeOrigin>,
}

impl Adapter {
    fn new(store: InMemoryStorage) -> Self {
        let config = VMConfig {
            verifier: VerifierConfig {
                max_dependency_depth: Some(100),
                ..Default::default()
            },
            ..Default::default()
        };
        let runtime = MoveRuntime::new(NativeFunctions::empty_for_testing().unwrap(), config);
        let vm = Arc::new(RwLock::new(
            InMemoryTestAdapter::new_with_runtime_and_storage(runtime, store),
        ));
        let linkage = LinkageContext::new(BTreeMap::new());
        Self {
            store: {
                RelinkingStore {
                    linkage,
                    type_origin: vec![],
                }
            },
            runtime_adapter: vm,
        }
    }

    fn with_linkage(
        &self,
        linkage: BTreeMap<OriginalId, VersionId>,
        type_origin: Vec<((&IdentStr, &IdentStr), VersionId)>,
    ) -> Self {
        Self {
            store: {
                let linkage = LinkageContext {
                    linkage_table: linkage,
                };
                let type_origin = type_origin
                    .into_iter()
                    .map(|((module, type_name), origin)| TypeOrigin {
                        module_name: module.to_owned(),
                        type_name: type_name.to_owned(),
                        origin_id: origin,
                    })
                    .collect();
                RelinkingStore {
                    linkage,
                    type_origin,
                }
            },
            runtime_adapter: self.runtime_adapter.clone(),
        }
    }

    fn publish_package(&mut self, mut pkg: StoredPackage) {
        if !self.store.linkage.linkage_table.is_empty() {
            pkg.linkage_context = self.store.linkage.clone();
        }
        if !self.store.type_origin.is_empty() {
            pkg.type_origin_table = self.store.type_origin.clone();
        }
        let runtime_id = pkg.original_id;
        self.runtime_adapter
            .write()
            .publish_package(runtime_id, pkg.into_serialized_package())
            .unwrap_or_else(|e| panic!("failure publishing modules: {e:?}"));
    }

    fn publish_package_with_error(&mut self, mut pkg: StoredPackage) {
        if !self.store.linkage.linkage_table.is_empty() {
            pkg.linkage_context = self.store.linkage.clone();
        }
        if !self.store.type_origin.is_empty() {
            pkg.type_origin_table = self.store.type_origin.clone();
        }
        let runtime_id = pkg.original_id;
        self.runtime_adapter
            .write()
            .publish_package(runtime_id, pkg.into_serialized_package())
            .expect_err("publishing must fail");
    }

    fn load_type(&self, type_tag: &TypeTag) -> Type {
        self.load_type_can_fail(type_tag)
            .expect("Loading type should succeed")
    }

    fn load_type_can_fail(&self, type_tag: &TypeTag) -> VMResult<Type> {
        let vm = self.runtime_adapter.write();
        let session = vm.make_vm(self.store.linkage.clone()).unwrap();
        session.load_type(type_tag)
    }

    fn compute_depth_of_datatype(
        &self,
        module_id: &ModuleId,
        struct_name: &IdentStr,
    ) -> DepthFormula {
        let vm = self.runtime_adapter.write();
        let mut session = vm.make_vm(self.store.linkage.clone()).unwrap();
        session
            .virtual_tables
            .calculate_depth_of_type(&VirtualTableKey {
                package_key: *module_id.address(),
                inner_pkg_key: IntraPackageKey {
                    module_name: intern_ident_str(module_id.name()).unwrap(),
                    member_name: intern_ident_str(struct_name).unwrap(),
                },
            })
            .expect("computing depth of datatype should succeed")
    }

    fn get_type_tag(&self, ty: &Type) -> TypeTag {
        let vm = self.runtime_adapter.write();
        let session = vm.make_vm(self.store.linkage.clone()).unwrap();
        session
            .virtual_tables
            .type_to_type_tag(ty)
            .expect("Converting to type tag should succeed")
    }

    fn call_functions(&self, functions: &[(ModuleId, Identifier)]) {
        for (module_id, name) in functions {
            println!("calling {name}");
            self.call_function(module_id, name);
        }
    }

    fn call_functions_async(&self, functions: Vec<(ModuleId, Identifier)>, reps: usize) {
        let mut children = vec![];
        for _ in 0..reps {
            for (module_id, name) in functions.clone() {
                let vm = self.runtime_adapter.clone();
                let data_store = self.store.clone();
                children.push(thread::spawn(move || {
                    let bind = vm.write();
                    let mut session = bind.make_vm(data_store.linkage.clone()).unwrap();
                    session
                        .execute_function_bypass_visibility(
                            &module_id,
                            &name,
                            vec![],
                            Vec::<Vec<u8>>::new(),
                            &mut UnmeteredGasMeter,
                            None,
                        )
                        .unwrap_or_else(|_| {
                            panic!("Failure executing {:?}::{:?}", module_id, name)
                        });
                }));
            }
        }
        for child in children {
            let _ = child.join();
        }
    }

    fn call_function_with_return(&self, module: &ModuleId, name: &IdentStr) -> Vec<MoveValue> {
        self.call_function(module, name)
            .return_values
            .into_iter()
            .map(|(bytes, ty)| {
                MoveValue::simple_deserialize(&bytes[..], &ty)
                    .expect("Can't deserialize return value")
            })
            .collect()
    }

    fn validate_linkage_with_err(&self) {
        let vm = self.runtime_adapter.write();
        let Err(_) = vm.make_vm(self.store.linkage.clone()) else {
            panic!("Should fail to make VM since function is missing");
        };
    }

    fn call_function(&self, module: &ModuleId, name: &IdentStr) -> SerializedReturnValues {
        let vm = self.runtime_adapter.write();
        let mut session = vm.make_vm(self.store.linkage.clone()).unwrap();
        session
            .execute_function_bypass_visibility(
                module,
                name,
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
                None,
            )
            .unwrap_or_else(|e| panic!("Failure executing {module:?}::{name:?}: {e:#?}"))
    }
}

fn get_fixture(fixture: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "move_packages", fixture]);
    path.to_string_lossy().into_owned()
}

fn get_loader_tests_modules() -> StoredPackage {
    let mut x = compile_packages_in_file(&get_fixture("loader_tests_modules.move"), &[]);
    assert!(x.len() == 1);
    x.pop().unwrap()
}

fn get_depth_tests_modules() -> StoredPackage {
    let mut x = compile_packages_in_file(&get_fixture("depth_tests_modules.move"), &[]);
    assert!(x.len() == 1);
    x.pop().unwrap()
}

fn get_relinker_tests_modules_with_deps<'s>(
    original_id: OriginalId,
    version_id: VersionId,
    module: &'s str,
    deps: impl IntoIterator<Item = &'s str>,
) -> anyhow::Result<StoredPackage> {
    fn fixture_string_path(module: &str) -> String {
        get_fixture(&format!("rt_{module}.move"))
    }

    let (_, units) = Compiler::from_files(
        None,
        vec![fixture_string_path(module)],
        deps.into_iter().map(fixture_string_path).collect(),
        BTreeMap::<String, _>::new(),
    )
    .build_and_report()?;

    let modules = expect_modules(units)
        .filter(|m| *m.self_id().address() == original_id)
        .collect();
    Ok(StoredPackage::from_modules_for_testing(version_id, modules).unwrap())
}

#[test]
fn load() {
    let data_store = InMemoryStorage::new();

    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);
    let pkg = get_loader_tests_modules();
    adapter.publish_package(pkg);
    // calls all functions sequentially
    adapter.call_functions(&ADDR2_FUNCTIONS);
}

#[test]
fn test_depth() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);
    let modules = get_depth_tests_modules();
    let structs = vec![
        (
            "A",
            "Box",
            DepthFormula {
                terms: vec![(0, 1)],
                constant: None,
            },
        ),
        (
            "A",
            "Box3",
            DepthFormula {
                terms: vec![(0, 3)],
                constant: None,
            },
        ),
        (
            "A",
            "Box7",
            DepthFormula {
                terms: vec![(0, 7)],
                constant: None,
            },
        ),
        (
            "A",
            "Box15",
            DepthFormula {
                terms: vec![(0, 15)],
                constant: None,
            },
        ),
        (
            "A",
            "Box31",
            DepthFormula {
                terms: vec![(0, 31)],
                constant: None,
            },
        ),
        (
            "A",
            "Box63",
            DepthFormula {
                terms: vec![(0, 63)],
                constant: None,
            },
        ),
        (
            "A",
            "Box127",
            DepthFormula {
                terms: vec![(0, 127)],
                constant: None,
            },
        ),
        (
            "A",
            "S",
            DepthFormula {
                terms: vec![],
                constant: Some(3),
            },
        ),
        (
            "B",
            "S",
            DepthFormula {
                terms: vec![],
                constant: Some(2),
            },
        ),
        (
            "C",
            "S",
            DepthFormula {
                terms: vec![],
                constant: Some(2),
            },
        ),
        (
            "D",
            "S",
            DepthFormula {
                terms: vec![],
                constant: Some(3),
            },
        ),
        (
            "E",
            "S",
            DepthFormula {
                terms: vec![(0, 2)],
                constant: Some(3),
            },
        ),
        (
            "F",
            "S",
            DepthFormula {
                terms: vec![(0, 1)],
                constant: Some(2),
            },
        ),
        (
            "G",
            "S",
            DepthFormula {
                terms: vec![(0, 5), (1, 3)],
                constant: Some(6),
            },
        ),
        (
            "H",
            "S",
            DepthFormula {
                terms: vec![(0, 2), (1, 4)],
                constant: Some(5),
            },
        ),
        (
            "I",
            "L",
            DepthFormula {
                terms: vec![(0, 2)],
                constant: Some(4),
            },
        ),
        (
            "I",
            "G",
            DepthFormula {
                terms: vec![],
                constant: Some(3),
            },
        ),
        (
            "I",
            "H",
            DepthFormula {
                terms: vec![(0, 1)],
                constant: Some(2),
            },
        ),
        (
            "I",
            "E",
            DepthFormula {
                terms: vec![(0, 2)],
                constant: Some(3),
            },
        ),
        (
            "I",
            "F",
            DepthFormula {
                terms: vec![(0, 1)],
                constant: Some(2),
            },
        ),
        (
            "I",
            "S",
            DepthFormula {
                terms: vec![(0, 2), (1, 7)],
                constant: Some(9),
            },
        ),
        (
            "I",
            "LL",
            DepthFormula {
                terms: vec![(1, 2)],
                constant: Some(4),
            },
        ),
        (
            "I",
            "N",
            DepthFormula {
                terms: vec![],
                constant: Some(2),
            },
        ),
    ];
    adapter.publish_package(modules);
    // loads all structs sequentially
    for (module_name, type_name, expected_depth) in structs.iter() {
        let computed_depth = &adapter.compute_depth_of_datatype(
            &ModuleId::new(ADDR2, Identifier::new(module_name.to_string()).unwrap()),
            ident_str!(type_name),
        );
        assert_eq!(computed_depth, expected_depth);
    }
}

#[test]
fn load_concurrent() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);
    let modules = get_loader_tests_modules();
    adapter.publish_package(modules);
    // makes 15 threads
    adapter.call_functions_async(ADDR2_FUNCTIONS.clone(), 3);
}

#[test]
fn load_concurrent_many() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);
    let modules = get_loader_tests_modules();
    adapter.publish_package(modules);
    // makes 150 threads
    adapter.call_functions_async(ADDR2_FUNCTIONS.clone(), 30);
}

#[test]
fn relink() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let a0 = ModuleId::new(ADDR4, ident_str!("a").to_owned());
    let b0 = ModuleId::new(ADDR3, ident_str!("b").to_owned());
    let c0 = ModuleId::new(ADDR2, ident_str!("c").to_owned());
    let c1 = ModuleId::new(ADDR5, ident_str!("c").to_owned());

    let c0_modules = get_relinker_tests_modules_with_deps(ADDR2, ADDR2, "c_v0", []).unwrap();
    let c1_modules =
        get_relinker_tests_modules_with_deps(ADDR2, *c1.address(), "c_v1", []).unwrap();
    let b0_modules = get_relinker_tests_modules_with_deps(ADDR3, ADDR3, "b_v0", ["c_v0"]).unwrap();
    let a0_modules =
        get_relinker_tests_modules_with_deps(ADDR4, ADDR4, "a_v0", ["b_v0", "c_v1"]).unwrap();

    // Publish the first version of C, and B which is published depending on it.
    adapter.publish_package(c0_modules);
    adapter
        .with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![])
        .publish_package(b0_modules);

    assert_eq!(
        vec![MoveValue::U64(42 + 1)],
        adapter
            .with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3),]), vec![],)
            .call_function_with_return(&b0, ident_str!("b")),
    );

    let mut adapter = adapter.with_linkage(
        /* linkage */ BTreeMap::from_iter([(ADDR2, ADDR5)]),
        /* type origin */
        vec![
            ((c0.name(), ident_str!("S")), *c0.address()),
            ((c1.name(), ident_str!("R")), *c1.address()),
        ],
    );

    // Publish the next version of C, and then A which depends on the new version of C, but also B.
    // B will be relinked to use C when executed in the adapter relinking against A.
    adapter.publish_package(c1_modules);
    let mut adapter = adapter.with_linkage(
        BTreeMap::from([(ADDR2, ADDR5), (ADDR3, ADDR3), (ADDR4, ADDR4)]),
        vec![],
    );
    adapter.publish_package(a0_modules);

    assert_eq!(
        vec![MoveValue::U64(44 + 43 + 1)],
        adapter.call_function_with_return(&a0, ident_str!("a")),
    );
}

#[test]
fn relink_publish_err() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let c0_modules = get_relinker_tests_modules_with_deps(ADDR2, ADDR2, "c_v0", []).unwrap();
    let b1_modules = get_relinker_tests_modules_with_deps(ADDR3, ADDR2, "b_v1", ["c_v1"]).unwrap();

    // B was built against the later version of C but published against the earlier version,
    // which should fail because a function is missing.
    adapter.publish_package(c0_modules);
    adapter
        .with_linkage(
            BTreeMap::from_iter([(ADDR2, ADDR2), (ADDR3, ADDR3)]),
            vec![],
        )
        .publish_package_with_error(b1_modules);
}

#[test]
fn relink_load_err() {
    let data_store = InMemoryStorage::new();
    let adapter = Adapter::new(data_store);

    let b0 = ModuleId::new(ADDR3, ident_str!("b").to_owned());
    let b1 = ModuleId::new(ADDR6, ident_str!("b").to_owned());
    let c0 = ModuleId::new(ADDR2, ident_str!("c").to_owned());
    let c1 = ModuleId::new(ADDR5, ident_str!("c").to_owned());

    let c0_modules = get_relinker_tests_modules_with_deps(ADDR2, ADDR2, "c_v0", []).unwrap();
    let c1_modules =
        get_relinker_tests_modules_with_deps(ADDR2, *c1.address(), "c_v1", []).unwrap();
    let b0_modules = get_relinker_tests_modules_with_deps(ADDR3, ADDR3, "b_v0", ["c_v0"]).unwrap();
    let b1_modules =
        get_relinker_tests_modules_with_deps(ADDR3, *b1.address(), "b_v1", ["c_v1"]).unwrap();

    // B v0 works with C v0
    adapter
        .with_linkage(BTreeMap::from([(*c0.address(), *c0.address())]), vec![])
        .publish_package(c0_modules);
    let mut adapter = adapter.with_linkage(
        BTreeMap::from([
            (*c0.address(), *c0.address()),
            (*b0.address(), *b0.address()),
        ]),
        vec![],
    );
    adapter.publish_package(b0_modules);

    assert_eq!(
        vec![MoveValue::U64(42 + 1)],
        adapter.call_function_with_return(&b0, ident_str!("b")),
    );

    adapter
        .with_linkage(
            /* linkage */
            BTreeMap::from_iter([(*c0.address(), *c1.address())]),
            /* type origin */
            vec![
                ((c0.name(), ident_str!("S")), *c0.address()),
                ((c1.name(), ident_str!("R")), *c1.address()),
            ],
        )
        .publish_package(c1_modules);

    // B v1 works with C v1
    let mut adapter = adapter.with_linkage(
        /* linkage */
        BTreeMap::from_iter([
            (*b0.address(), *b1.address()),
            (*c0.address(), *c1.address()),
        ]),
        /* type origin */
        vec![
            ((b0.name(), ident_str!("S")), *b1.address()),
            ((c0.name(), ident_str!("S")), *c0.address()),
            ((c1.name(), ident_str!("R")), *c1.address()),
        ],
    );
    adapter.publish_package(b1_modules);

    assert_eq!(
        vec![MoveValue::U64(44 * 43)],
        adapter.call_function_with_return(&b0, ident_str!("b")),
    );

    // But B v1 *does not* work with C v0
    adapter
        .with_linkage(
            /* linkage */
            BTreeMap::from_iter([
                (*b0.address(), *b1.address()),
                (*c0.address(), *c0.address()),
            ]),
            /* type origin */
            vec![
                ((b0.name(), ident_str!("S")), *b1.address()),
                ((c0.name(), ident_str!("S")), *c0.address()),
                ((c1.name(), ident_str!("R")), *c1.address()),
            ],
        )
        .validate_linkage_with_err();
}

#[test]
fn relink_type_identity() {
    let data_store = InMemoryStorage::new();
    let adapter = Adapter::new(data_store);

    let b0 = ModuleId::new(ADDR3, ident_str!("b").to_owned());
    let c0 = ModuleId::new(ADDR2, ident_str!("c").to_owned());
    let b1 = ModuleId::new(ADDR6, ident_str!("b").to_owned());
    let c1 = ModuleId::new(ADDR5, ident_str!("c").to_owned());
    let c0_modules = get_relinker_tests_modules_with_deps(ADDR2, ADDR2, "c_v0", []).unwrap();
    let c1_modules =
        get_relinker_tests_modules_with_deps(ADDR2, *c1.address(), "c_v1", []).unwrap();
    let b1_modules =
        get_relinker_tests_modules_with_deps(ADDR3, *b1.address(), "b_v1", ["c_v1"]).unwrap();

    let mut adapter =
        adapter.with_linkage(BTreeMap::from([(*c0.address(), *c0.address())]), vec![]);
    adapter.publish_package(c0_modules);
    let c0_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());

    adapter
        .with_linkage(
            /* linkage */
            BTreeMap::from_iter([(*c0.address(), *c1.address())]),
            /* type origin */
            vec![
                ((c0.name(), ident_str!("S")), *c0.address()),
                ((c1.name(), ident_str!("R")), *c1.address()),
            ],
        )
        .publish_package(c1_modules);

    let mut adapter = adapter.with_linkage(
        /* linkage */
        BTreeMap::from_iter([
            (*b0.address(), *b1.address()),
            (*c0.address(), *c1.address()),
        ]),
        /* type origin */
        vec![
            ((b0.name(), ident_str!("S")), *b1.address()),
            ((c0.name(), ident_str!("S")), *c0.address()),
            ((c1.name(), ident_str!("R")), *c1.address()),
        ],
    );
    adapter.publish_package(b1_modules);

    let c1_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());
    // Cannot use runtime ID for type!
    assert!(adapter
        .load_type_can_fail(&TypeTag::from_str("0x3::b::S").unwrap())
        .is_err());
    let b1_s = adapter.load_type(&TypeTag::from_str("0x6::b::S").unwrap());

    assert_eq!(c0_s, c1_s);
    assert_ne!(c1_s, b1_s);
}

#[test]
fn relink_defining_module_successive() {
    let c0 = ModuleId::new(ADDR2, ident_str!("c").to_owned());
    let c1 = ModuleId::new(ADDR5, ident_str!("c").to_owned());
    let c2 = ModuleId::new(ADDR6, ident_str!("c").to_owned());
    // This test simulates building up a sequence of upgraded packages over a number of publishes
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store).with_linkage(
        BTreeMap::from([(ADDR2, ADDR2)]),
        vec![((c0.name(), ident_str!("S")), *c0.address())],
    );

    let c0_modules = get_relinker_tests_modules_with_deps(ADDR2, ADDR2, "c_v0", []).unwrap();
    let c1_modules =
        get_relinker_tests_modules_with_deps(ADDR2, *c1.address(), "c_v1", []).unwrap();
    let c2_modules =
        get_relinker_tests_modules_with_deps(ADDR2, *c2.address(), "c_v2", []).unwrap();

    adapter.publish_package(c0_modules);
    let c0_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());

    let mut adapter = adapter.with_linkage(
        /* linkage */ BTreeMap::from_iter([(*c0.address(), *c1.address())]),
        /* type origin */
        vec![
            ((c0.name(), ident_str!("S")), *c0.address()),
            ((c1.name(), ident_str!("R")), *c1.address()),
        ],
    );

    adapter.publish_package(c1_modules);
    let c1_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());
    assert!(adapter
        .load_type_can_fail(&TypeTag::from_str("0x2::c::R").unwrap())
        .is_err());
    let c1_r = adapter.load_type(&TypeTag::from_str("0x5::c::R").unwrap());

    let mut adapter = adapter.with_linkage(
        /* linkage */ BTreeMap::from_iter([(*c0.address(), *c2.address())]),
        /* type origin */
        vec![
            ((c0.name(), ident_str!("S")), *c0.address()),
            ((c1.name(), ident_str!("R")), *c1.address()),
            ((c0.name(), ident_str!("Q")), *c2.address()),
        ],
    );

    adapter.publish_package(c2_modules);
    let c2_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());
    assert!(adapter
        .load_type_can_fail(&TypeTag::from_str("0x2::c::R").unwrap())
        .is_err());
    assert!(adapter
        .load_type_can_fail(&TypeTag::from_str("0x2::c::Q").unwrap())
        .is_err());
    // Types must be loaded by defining ID. It is the adapter's responsibility to ensure that type
    // tags are loaded with the correct defining module.
    let c2_r = adapter.load_type(&TypeTag::from_str("0x5::c::R").unwrap());
    let c2_q = adapter.load_type(&TypeTag::from_str("0x6::c::Q").unwrap());

    for s in &[c0_s, c1_s, c2_s] {
        let TypeTag::Struct(st) = adapter.get_type_tag(s) else {
            panic!("Not a struct: {s:?}")
        };

        assert_eq!(st.module_id(), c0);
    }

    for r in &[c1_r, c2_r] {
        let TypeTag::Struct(st) = adapter.get_type_tag(r) else {
            panic!("Not a struct: {r:?}")
        };

        assert_eq!(st.module_id(), c1);
    }

    let TypeTag::Struct(st) = adapter.get_type_tag(&c2_q) else {
        panic!("Not a struct: {c2_q:?}")
    };

    assert_eq!(st.module_id(), c2);
}

#[test]
fn relink_defining_module_oneshot() {
    // Simulates the loader being made aware of the final package in a sequence of upgrades (perhaps
    // a previous instance of the VM and loader participated in the publishing of previous versions)
    // but still needing to correctly set-up the defining modules for the types in the latest
    // version of the package, based on the linkage table at the time of loading/publishing:

    let data_store = InMemoryStorage::new();

    let c0 = ModuleId::new(ADDR2, ident_str!("c").to_owned());
    let c1 = ModuleId::new(ADDR5, ident_str!("c").to_owned());
    let c2 = ModuleId::new(ADDR6, ident_str!("c").to_owned());

    let c2_modules =
        get_relinker_tests_modules_with_deps(ADDR2, *c2.address(), "c_v2", []).unwrap();

    let mut adapter = Adapter::new(data_store).with_linkage(
        /* linkage */ BTreeMap::from_iter([(*c0.address(), *c2.address())]),
        /* type origin */
        vec![
            ((c0.name(), ident_str!("S")), *c0.address()),
            ((c0.name(), ident_str!("R")), *c1.address()),
            ((c0.name(), ident_str!("Q")), *c2.address()),
        ],
    );

    adapter.publish_package(c2_modules);
    let s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());
    let r = adapter.load_type(&TypeTag::from_str("0x5::c::R").unwrap());
    let q = adapter.load_type(&TypeTag::from_str("0x6::c::Q").unwrap());

    let TypeTag::Struct(s) = adapter.get_type_tag(&s) else {
        panic!("Not a struct: {s:?}")
    };

    let TypeTag::Struct(r) = adapter.get_type_tag(&r) else {
        panic!("Not a struct: {r:?}")
    };

    let TypeTag::Struct(q) = adapter.get_type_tag(&q) else {
        panic!("Not a struct: {q:?}")
    };

    assert_eq!(s.module_id(), c0);
    assert_eq!(r.module_id(), c1);
    assert_eq!(q.module_id(), c2);
}

// TODO(vm-rewrite): Update and re-enable this test once we add failpoints to the VM
// #[test]
// fn relink_defining_module_cleanup() {
//     // If loading fails for a module that pulls in a module that was defined at an earlier version
//     // of the package, roll-back should occur cleanly.
//     let data_store = InMemoryStorage::new();
//
//     let c0 = ModuleId::new(ADDR2, ident_str!("c").to_owned());
//     let b0 = ModuleId::new(ADDR3, ident_str!("b").to_owned());
//     let b1 = ModuleId::new(ADDR6, ident_str!("b").to_owned());
//
//     let adapter = Adapter::new(data_store);
//     let c0_modules = get_relinker_tests_modules_with_deps(ADDR2, ADDR2, "c_v0", []).unwrap();
//     let b1_modules = get_relinker_tests_modules_with_deps(ADDR3, *b1.address(), "b_v1", ["c_v1"]).unwrap();
//
//     // B was built against the later version of C but published against the earlier version,
//     // which should fail because a function is missing.
//     adapter
//         .linkage(
//             *c0.address(),
//             /* linkage */
//             BTreeMap::from_iter([(*c0.address(), *c0.address())]),
//             /* type origin */
//             BTreeMap::from_iter([((c0.clone(), ident_str!("S").to_owned()), c0.clone())]),
//         )
//         .publish_modules(c0_modules);
//
//     // Somehow dependency verification fails, and the publish succeeds.
//     fail::cfg("verifier-failpoint-4", "100%return").unwrap();
//     let mut adapter = adapter.linkage(
//         *b0.address(),
//         /* linkage */
//         BTreeMap::from_iter([
//             (*b0.address(), *b1.address()),
//             (*c0.address(), *c0.address()),
//         ]),
//         /* type origin */
//         BTreeMap::from_iter([
//             ((c0.clone(), ident_str!("S").to_owned()), c0.clone()),
//             ((b0.clone(), ident_str!("S").to_owned()), b1.clone()),
//         ]),
//     );
//     adapter.publish_modules(b1_modules);
//
//     // This call should fail to load the module and rollback cleanly
//     adapter.call_function_with_error(&b0, ident_str!("b"));
//
//     // Restore old behavior of failpoint
//     fail::cfg("verifier-failpoint-4", "off").unwrap();
// }

#[test]
fn publish_bundle_and_load() {
    let data_store = InMemoryStorage::new();
    let adapter = Adapter::new(data_store);

    let a0 = ModuleId::new(ADDR4, ident_str!("a").to_owned());
    let c1_modules = get_relinker_tests_modules_with_deps(ADDR2, ADDR2, "c_v1", []).unwrap();
    let b0_modules = get_relinker_tests_modules_with_deps(ADDR3, ADDR3, "b_v0", ["c_v0"]).unwrap();
    let a0_modules =
        get_relinker_tests_modules_with_deps(ADDR4, ADDR4, "a_v0", ["b_v0", "c_v1"]).unwrap();

    adapter
        .with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![])
        .publish_package(c1_modules);

    adapter
        .with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![])
        .publish_package(b0_modules);

    let mut adapter = adapter.with_linkage(
        BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3), (ADDR4, ADDR4)]),
        vec![],
    );
    adapter.publish_package(a0_modules);

    assert_eq!(
        vec![MoveValue::U64(44 + 43 + 1)],
        adapter.call_function_with_return(&a0, ident_str!("a")),
    );
}

#[test]
fn publish_bundle_with_err_retry() {
    let data_store = InMemoryStorage::new();
    let adapter = Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let a0 = ModuleId::new(ADDR4, ident_str!("a").to_owned());
    let c0_modules = get_relinker_tests_modules_with_deps(ADDR2, ADDR2, "c_v0", []).unwrap();
    let c1_modules = get_relinker_tests_modules_with_deps(ADDR2, ADDR5, "c_v1", []).unwrap();
    let b0_modules = get_relinker_tests_modules_with_deps(ADDR3, ADDR3, "b_v0", ["c_v0"]).unwrap();
    let a0_modules =
        get_relinker_tests_modules_with_deps(ADDR4, ADDR4, "a_v0", ["b_v0", "c_v1"]).unwrap();

    adapter
        .with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![])
        .publish_package(c0_modules);

    adapter
        .with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![])
        .publish_package(b0_modules);

    // Publishing the bundle should fail, because `a0` should not link with `c0`.
    adapter
        .with_linkage(
            BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3), (ADDR4, ADDR4)]),
            vec![],
        )
        .publish_package_with_error(a0_modules.clone());

    // publish the upgrade of c0 to ADDR5
    adapter
        .with_linkage(BTreeMap::from([(ADDR2, ADDR5)]), vec![])
        .publish_package(c1_modules);

    let mut adapter = adapter.with_linkage(
        BTreeMap::from([(ADDR2, ADDR5), (ADDR3, ADDR3), (ADDR4, ADDR4)]),
        vec![],
    );

    // Try again and everything should publish successfully (in particular, the failed publish
    // will not leave behind modules in the loader).
    adapter.publish_package(a0_modules);

    assert_eq!(
        vec![MoveValue::U64(44 + 43 + 1)],
        adapter.call_function_with_return(&a0, ident_str!("a")),
    );
}

#[test]
fn deep_dependency_list_0() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 350u64;
    dependency_chain(1, max, &mut modules);
    let pkg = StoredPackage::from_modules_for_testing(ADDR2, modules).unwrap();
    adapter.publish_package(pkg);

    let mut adapter =
        adapter.with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![]);
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(ADDR3, name, (ADDR2, deps));
    let pkg = StoredPackage::from_modules_for_testing(ADDR3, vec![module]).unwrap();
    adapter.publish_package(pkg);
}

#[test]
fn deep_dependency_list_1() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 101u64;
    dependency_chain(1, max, &mut modules);
    let pkg = StoredPackage::from_modules_for_testing(ADDR2, modules).unwrap();
    adapter.publish_package(pkg);

    let mut adapter =
        adapter.with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![]);
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(ADDR3, name, (ADDR2, deps));
    let pkg = StoredPackage::from_modules_for_testing(ADDR3, vec![module]).unwrap();
    adapter.publish_package(pkg);
}

#[test]
fn deep_dependency_list_ok_0() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 100u64;
    dependency_chain(1, max, &mut modules);
    let pkg = StoredPackage::from_modules_for_testing(ADDR2, modules).unwrap();
    adapter.publish_package(pkg);

    let mut adapter =
        adapter.with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![]);
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(ADDR3, name, (ADDR2, deps));
    let pkg = StoredPackage::from_modules_for_testing(ADDR3, vec![module]).unwrap();
    adapter.publish_package(pkg);
}

#[test]
fn deep_dependency_list_ok_1() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 30u64;
    dependency_chain(1, max, &mut modules);
    let pkg = StoredPackage::from_modules_for_testing(ADDR2, modules).unwrap();
    adapter.publish_package(pkg);

    let mut adapter =
        adapter.with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![]);
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(ADDR3, name, (ADDR2, deps));
    let pkg = StoredPackage::from_modules_for_testing(ADDR3, vec![module]).unwrap();
    adapter.publish_package(pkg);
}

#[test]
fn deep_dependency_tree_0() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let mut modules = vec![];

    // create a tree of dependencies
    let width = 5u64;
    let height = 101u64;
    dependency_tree(width, height, &mut modules);
    let pkg = StoredPackage::from_modules_for_testing(ADDR2, modules).unwrap();
    adapter.publish_package(pkg);

    // use one of the module in the tree
    let mut adapter =
        adapter.with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![]);
    let name = "ASome".to_string();
    let dep_name = format!("A_{}_{}", height - 1, width - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(ADDR3, name, (ADDR2, deps));
    let pkg = StoredPackage::from_modules_for_testing(ADDR3, vec![module]).unwrap();
    adapter.publish_package(pkg);
}

#[test]
fn deep_dependency_tree_1() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let mut modules = vec![];

    // create a tree of dependencies
    let width = 3u64;
    let height = 350u64;
    dependency_tree(width, height, &mut modules);
    let pkg = StoredPackage::from_modules_for_testing(ADDR2, modules).unwrap();
    adapter.publish_package(pkg);

    // use one of the module in the tree
    let mut adapter =
        adapter.with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![]);
    let name = "ASome".to_string();
    let dep_name = format!("A_{}_{}", height - 1, width - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(ADDR3, name, (ADDR2, deps));
    let pkg = StoredPackage::from_modules_for_testing(ADDR3, vec![module]).unwrap();
    adapter.publish_package(pkg);
}

#[test]
fn deep_dependency_tree_ok_0() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let mut modules = vec![];

    // create a tree of dependencies
    let width = 10u64;
    let height = 20u64;
    dependency_tree(width, height, &mut modules);
    let pkg = StoredPackage::from_modules_for_testing(ADDR2, modules).unwrap();
    adapter.publish_package(pkg);

    // use one of the module in the tree
    let mut adapter =
        adapter.with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![]);
    let name = "ASome".to_string();
    let dep_name = format!("A_{}_{}", height - 1, width - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(ADDR3, name, (ADDR2, deps));
    let pkg = StoredPackage::from_modules_for_testing(ADDR3, vec![module]).unwrap();
    adapter.publish_package(pkg);
}

#[test]
fn deep_dependency_tree_ok_1() {
    let data_store = InMemoryStorage::new();
    let mut adapter =
        Adapter::new(data_store).with_linkage(BTreeMap::from([(ADDR2, ADDR2)]), vec![]);

    let mut modules = vec![];

    // create a tree of dependencies
    let width = 3u64;
    let height = 100u64;
    dependency_tree(width, height, &mut modules);
    let pkg = StoredPackage::from_modules_for_testing(ADDR2, modules).unwrap();
    adapter.publish_package(pkg);

    // use one of the module in the tree
    let mut adapter =
        adapter.with_linkage(BTreeMap::from([(ADDR2, ADDR2), (ADDR3, ADDR3)]), vec![]);
    let name = "ASome".to_string();
    let dep_name = format!("A_{}_{}", height - 1, width - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(ADDR3, name, (ADDR2, deps));
    let pkg = StoredPackage::from_modules_for_testing(ADDR3, vec![module]).unwrap();
    adapter.publish_package(pkg);
}

fn leaf_module(name: &str) -> CompiledModule {
    let mut module = empty_module();
    module.identifiers[0] = Identifier::new(name).unwrap();
    module.address_identifiers[0] = ADDR2;
    module
}

// Create a list of dependent modules
fn dependency_chain(start: u64, end: u64, modules: &mut Vec<CompiledModule>) {
    let module = leaf_module("A0");
    modules.push(module);

    for i in start..end {
        let name = format!("A{}", i);
        let dep_name = format!("A{}", i - 1);
        let deps = vec![dep_name];
        let module = empty_module_with_dependencies(ADDR2, name, (ADDR2, deps));
        modules.push(module);
    }
}

// Create a tree (well a forest or DAG really) of dependent modules
fn dependency_tree(width: u64, height: u64, modules: &mut Vec<CompiledModule>) {
    let mut deps = vec![];
    for i in 0..width {
        let name = format!("A_{}_{}", 0, i);
        let module = leaf_module(name.as_str());
        deps.push(name);
        modules.push(module);
    }
    for i in 1..height {
        let mut new_deps = vec![];
        for j in 0..width {
            let name = format!("A_{}_{}", i, j);
            let module = empty_module_with_dependencies(ADDR2, name.clone(), (ADDR2, deps.clone()));
            new_deps.push(name);
            modules.push(module);
        }
        deps = new_deps;
    }
}

// Create a module that uses (depends on) the list of given modules
fn empty_module_with_dependencies(
    address: VersionId,
    name: String,
    deps: (VersionId, Vec<String>),
) -> CompiledModule {
    let mut module = empty_module();
    module.address_identifiers[0] = address;
    module.identifiers[0] = Identifier::new(name).unwrap();
    let idx = if address == deps.0 {
        0
    } else {
        module.address_identifiers.push(deps.0);
        1
    };
    for dep in deps.1 {
        module.identifiers.push(Identifier::new(dep).unwrap());
        module.module_handles.push(ModuleHandle {
            address: AddressIdentifierIndex(idx),
            name: IdentifierIndex((module.identifiers.len() - 1) as TableIndex),
        });
    }
    module
}
