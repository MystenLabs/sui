// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Simplifies logic around re-using ModuleIds.
#![allow(clippy::redundant_clone)]

use crate::compiler::{compile_modules_in_file, expect_modules};
use move_binary_format::{
    file_format::{
        empty_module, AddressIdentifierIndex, IdentifierIndex, ModuleHandle, TableIndex,
    },
    CompiledModule,
};
use move_compiler::Compiler;
use move_core_types::{
    account_address::AccountAddress,
    effects::ChangeSet,
    ident_str,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
    runtime_value::MoveValue,
};
use move_vm_config::{runtime::VMConfig, verifier::VerifierConfig};
use move_vm_runtime::{move_vm::MoveVM, session::SerializedReturnValues};
use move_vm_test_utils::InMemoryStorage;
use move_vm_types::{
    gas::UnmeteredGasMeter,
    loaded_data::runtime_types::{CachedDatatype, DepthFormula, Type},
};

use std::{collections::BTreeMap, path::PathBuf, str::FromStr, sync::Arc, thread};

const DEFAULT_ACCOUNT: AccountAddress = AccountAddress::TWO;
const UPGRADE_ACCOUNT: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 3u8;
    AccountAddress::new(address)
};
const UPGRADE_ACCOUNT_2: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 4u8;
    AccountAddress::new(address)
};

struct Adapter {
    store: RelinkingStore,
    vm: Arc<MoveVM>,
    functions: Vec<(ModuleId, Identifier)>,
}

#[derive(Clone)]
struct RelinkingStore {
    store: InMemoryStorage,
    context: AccountAddress,
    linkage: BTreeMap<ModuleId, ModuleId>,
    type_origin: BTreeMap<(ModuleId, Identifier), ModuleId>,
}

impl Adapter {
    fn new(store: InMemoryStorage) -> Self {
        let functions = vec![
            (
                ModuleId::new(DEFAULT_ACCOUNT, Identifier::new("A").unwrap()),
                Identifier::new("entry_a").unwrap(),
            ),
            (
                ModuleId::new(DEFAULT_ACCOUNT, Identifier::new("D").unwrap()),
                Identifier::new("entry_d").unwrap(),
            ),
            (
                ModuleId::new(DEFAULT_ACCOUNT, Identifier::new("E").unwrap()),
                Identifier::new("entry_e").unwrap(),
            ),
            (
                ModuleId::new(DEFAULT_ACCOUNT, Identifier::new("F").unwrap()),
                Identifier::new("entry_f").unwrap(),
            ),
            (
                ModuleId::new(DEFAULT_ACCOUNT, Identifier::new("C").unwrap()),
                Identifier::new("just_c").unwrap(),
            ),
        ];
        let config = VMConfig {
            verifier: VerifierConfig {
                max_dependency_depth: Some(100),
                ..Default::default()
            },
            ..Default::default()
        };
        Self {
            store: RelinkingStore::new(store),
            vm: Arc::new(MoveVM::new_with_config(vec![], config).unwrap()),
            functions,
        }
    }

    fn fresh(self) -> Self {
        let config = VMConfig {
            verifier: VerifierConfig {
                max_dependency_depth: Some(100),
                ..Default::default()
            },
            ..Default::default()
        };
        Self {
            store: self.store,
            vm: Arc::new(MoveVM::new_with_config(vec![], config).unwrap()),
            functions: self.functions,
        }
    }

    fn relink(
        self,
        context: AccountAddress,
        linkage: BTreeMap<ModuleId, ModuleId>,
        type_origin: BTreeMap<(ModuleId, Identifier), ModuleId>,
    ) -> Self {
        Self {
            store: self.store.relink(context, linkage, type_origin),
            vm: self.vm,
            functions: self.functions,
        }
    }

    fn publish_modules(&mut self, modules: Vec<CompiledModule>) {
        let mut session = self.vm.new_session(&self.store);

        for module in modules {
            let mut binary = vec![];
            module.serialize(&mut binary).unwrap_or_else(|e| {
                panic!("failure in module serialization: {e:?}\n{:#?}", module)
            });
            session
                .publish_module(binary, DEFAULT_ACCOUNT, &mut UnmeteredGasMeter)
                .unwrap_or_else(|e| panic!("failure publishing module: {e:?}\n{:#?}", module));
        }
        let (changeset, _) = session.finish().0.expect("failure getting write set");
        self.store
            .apply(changeset)
            .expect("failure applying write set");
    }

    fn publish_modules_with_error(&mut self, modules: Vec<CompiledModule>) {
        let mut session = self.vm.new_session(&self.store);

        for module in modules {
            let mut binary = vec![];
            module.serialize(&mut binary).unwrap_or_else(|e| {
                panic!("failure in module serialization: {e:?}\n{:#?}", module)
            });
            session
                .publish_module(binary, DEFAULT_ACCOUNT, &mut UnmeteredGasMeter)
                .expect_err("publishing must fail");
        }
    }

    fn publish_module_bundle(&mut self, modules: Vec<CompiledModule>) {
        let mut session = self.vm.new_session(&self.store);
        let binaries: Vec<_> = modules
            .into_iter()
            .map(|module| {
                let mut binary = vec![];
                module.serialize(&mut binary).unwrap_or_else(|e| {
                    panic!("failure in module serialization: {e:?}\n{:#?}", module)
                });
                binary
            })
            .collect();

        session
            .publish_module_bundle(binaries, DEFAULT_ACCOUNT, &mut UnmeteredGasMeter)
            .unwrap_or_else(|e| panic!("failure publishing module bundle: {e:?}"));

        let (changeset, _) = session.finish().0.expect("failure getting write set");
        self.store
            .apply(changeset)
            .expect("failure applying write set");
    }

    fn publish_module_bundle_with_error(&mut self, modules: Vec<CompiledModule>) {
        let mut session = self.vm.new_session(&self.store);
        let binaries: Vec<_> = modules
            .into_iter()
            .map(|module| {
                let mut binary = vec![];
                module.serialize(&mut binary).unwrap_or_else(|e| {
                    panic!("failure in module serialization: {e:?}\n{:#?}", module)
                });
                binary
            })
            .collect();

        session
            .publish_module_bundle(binaries, DEFAULT_ACCOUNT, &mut UnmeteredGasMeter)
            .expect_err("publishing bundle must fail");
    }

    fn load_type(&self, type_tag: &TypeTag) -> Type {
        let session = self.vm.new_session(&self.store);
        session
            .load_type(type_tag)
            .expect("Loading type should succeed")
    }

    fn load_datatype(&self, module_id: &ModuleId, struct_name: &IdentStr) -> Arc<CachedDatatype> {
        let session = self.vm.new_session(&self.store);
        session
            .load_datatype(module_id, struct_name)
            .expect("Loading struct should succeed")
            .1
    }

    fn get_type_tag(&self, ty: &Type) -> TypeTag {
        let session = self.vm.new_session(&self.store);
        session
            .get_type_tag(ty)
            .expect("Converting to type tag should succeed")
    }

    fn call_functions(&self) {
        for (module_id, name) in &self.functions {
            self.call_function(module_id, name);
        }
    }

    fn call_functions_async(&self, reps: usize) {
        let mut children = vec![];
        for _ in 0..reps {
            for (module_id, name) in self.functions.clone() {
                let vm = self.vm.clone();
                let data_store = self.store.clone();
                children.push(thread::spawn(move || {
                    let mut session = vm.new_session(&data_store);
                    session
                        .execute_function_bypass_visibility(
                            &module_id,
                            &name,
                            vec![],
                            Vec::<Vec<u8>>::new(),
                            &mut UnmeteredGasMeter,
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

    fn call_function_with_error(&self, module: &ModuleId, name: &IdentStr) {
        let mut session = self.vm.new_session(&self.store);
        session
            .execute_function_bypass_visibility(
                module,
                name,
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
            )
            .expect_err("calling function must fail");
    }

    fn call_function(&self, module: &ModuleId, name: &IdentStr) -> SerializedReturnValues {
        let mut session = self.vm.new_session(&self.store);
        session
            .execute_function_bypass_visibility(
                module,
                name,
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
            )
            .unwrap_or_else(|e| panic!("Failure executing {module:?}::{name:?}: {e:#?}"))
    }
}

impl RelinkingStore {
    fn new(store: InMemoryStorage) -> Self {
        Self {
            store,
            context: AccountAddress::ZERO,
            linkage: BTreeMap::new(),
            type_origin: BTreeMap::new(),
        }
    }

    fn relink(
        self,
        context: AccountAddress,
        linkage: BTreeMap<ModuleId, ModuleId>,
        type_origin: BTreeMap<(ModuleId, Identifier), ModuleId>,
    ) -> Self {
        let Self { store, .. } = self;
        Self {
            store,
            context,
            linkage,
            type_origin,
        }
    }

    fn apply(&mut self, changeset: ChangeSet) -> anyhow::Result<()> {
        self.store.apply(changeset)
    }
}

/// Implemented by referencing the store's built-in data structures
impl LinkageResolver for RelinkingStore {
    type Error = ();

    fn link_context(&self) -> AccountAddress {
        self.context
    }

    /// Remaps `module_id` if it exists in the current linkage table, or returns it unchanged
    /// otherwise.
    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        Ok(self.linkage.get(module_id).unwrap_or(module_id).clone())
    }

    fn defining_module(
        &self,
        module_id: &ModuleId,
        struct_: &IdentStr,
    ) -> Result<ModuleId, Self::Error> {
        Ok(self
            .type_origin
            .get(&(module_id.clone(), struct_.to_owned()))
            .unwrap_or(module_id)
            .clone())
    }
}

/// Implement by forwarding to the underlying in memory storage
impl ModuleResolver for RelinkingStore {
    type Error = ();

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.store.get_module(id)
    }
}

/// Implement by forwarding to the underlying in memory storage
impl ResourceResolver for RelinkingStore {
    type Error = ();

    fn get_resource(
        &self,
        address: &AccountAddress,
        typ: &move_core_types::language_storage::StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.store.get_resource(address, typ)
    }
}

fn get_fixture(fixture: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "tests", fixture]);
    path
}

fn get_loader_tests_modules() -> Vec<CompiledModule> {
    compile_modules_in_file(&get_fixture("loader_tests_modules.move")).unwrap()
}

fn get_depth_tests_modules() -> Vec<CompiledModule> {
    compile_modules_in_file(&get_fixture("depth_tests_modules.move")).unwrap()
}

fn get_relinker_tests_modules_with_deps<'s>(
    module: &'s str,
    deps: impl IntoIterator<Item = &'s str>,
) -> anyhow::Result<Vec<CompiledModule>> {
    fn fixture_string_path(module: &str) -> String {
        get_fixture(&format!("relinking_tests_{module}.move"))
            .to_str()
            .unwrap()
            .to_string()
    }

    let (_, units) = Compiler::from_files(
        vec![fixture_string_path(module)],
        deps.into_iter().map(fixture_string_path).collect(),
        BTreeMap::<String, _>::new(),
    )
    .build_and_report()?;

    Ok(expect_modules(units).collect())
}

#[test]
fn load() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);
    let modules = get_loader_tests_modules();
    adapter.publish_modules(modules);
    // calls all functions sequentially
    adapter.call_functions();
}

#[test]
fn test_depth() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);
    let modules = get_depth_tests_modules();
    let structs = vec![
        (
            "A",
            "Box",
            Some(DepthFormula {
                terms: vec![(0, 1)],
                constant: None,
            }),
        ),
        (
            "A",
            "Box3",
            Some(DepthFormula {
                terms: vec![(0, 3)],
                constant: None,
            }),
        ),
        (
            "A",
            "Box7",
            Some(DepthFormula {
                terms: vec![(0, 7)],
                constant: None,
            }),
        ),
        (
            "A",
            "Box15",
            Some(DepthFormula {
                terms: vec![(0, 15)],
                constant: None,
            }),
        ),
        (
            "A",
            "Box31",
            Some(DepthFormula {
                terms: vec![(0, 31)],
                constant: None,
            }),
        ),
        (
            "A",
            "Box63",
            Some(DepthFormula {
                terms: vec![(0, 63)],
                constant: None,
            }),
        ),
        (
            "A",
            "Box127",
            Some(DepthFormula {
                terms: vec![(0, 127)],
                constant: None,
            }),
        ),
        (
            "A",
            "S",
            Some(DepthFormula {
                terms: vec![],
                constant: Some(3),
            }),
        ),
        (
            "B",
            "S",
            Some(DepthFormula {
                terms: vec![],
                constant: Some(2),
            }),
        ),
        (
            "C",
            "S",
            Some(DepthFormula {
                terms: vec![],
                constant: Some(2),
            }),
        ),
        (
            "D",
            "S",
            Some(DepthFormula {
                terms: vec![],
                constant: Some(3),
            }),
        ),
        (
            "E",
            "S",
            Some(DepthFormula {
                terms: vec![(0, 2)],
                constant: Some(3),
            }),
        ),
        (
            "F",
            "S",
            Some(DepthFormula {
                terms: vec![(0, 1)],
                constant: Some(2),
            }),
        ),
        (
            "G",
            "S",
            Some(DepthFormula {
                terms: vec![(0, 5), (1, 3)],
                constant: Some(6),
            }),
        ),
        (
            "H",
            "S",
            Some(DepthFormula {
                terms: vec![(0, 2), (1, 4)],
                constant: Some(5),
            }),
        ),
        (
            "I",
            "L",
            Some(DepthFormula {
                terms: vec![(0, 2)],
                constant: Some(4),
            }),
        ),
        (
            "I",
            "G",
            Some(DepthFormula {
                terms: vec![],
                constant: Some(3),
            }),
        ),
        (
            "I",
            "H",
            Some(DepthFormula {
                terms: vec![(0, 1)],
                constant: Some(2),
            }),
        ),
        (
            "I",
            "E",
            Some(DepthFormula {
                terms: vec![(0, 2)],
                constant: Some(3),
            }),
        ),
        (
            "I",
            "F",
            Some(DepthFormula {
                terms: vec![(0, 1)],
                constant: Some(2),
            }),
        ),
        (
            "I",
            "S",
            Some(DepthFormula {
                terms: vec![(0, 2), (1, 7)],
                constant: Some(9),
            }),
        ),
        (
            "I",
            "LL",
            Some(DepthFormula {
                terms: vec![(1, 2)],
                constant: Some(4),
            }),
        ),
        (
            "I",
            "N",
            Some(DepthFormula {
                terms: vec![],
                constant: Some(2),
            }),
        ),
    ];
    adapter.publish_modules(modules);
    // loads all structs sequentially
    for (module_name, type_name, expected_depth) in structs.iter() {
        let computed_depth = &adapter
            .load_datatype(
                &ModuleId::new(
                    DEFAULT_ACCOUNT,
                    Identifier::new(module_name.to_string()).unwrap(),
                ),
                ident_str!(type_name),
            )
            .depth;
        assert_eq!(computed_depth, expected_depth);
    }
}

#[test]
fn load_concurrent() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);
    let modules = get_loader_tests_modules();
    adapter.publish_modules(modules);
    // makes 15 threads
    adapter.call_functions_async(3);
}

#[test]
fn load_concurrent_many() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);
    let modules = get_loader_tests_modules();
    adapter.publish_modules(modules);
    // makes 150 threads
    adapter.call_functions_async(30);
}

#[test]
fn relink() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let a0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("a").to_owned());
    let b0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("b").to_owned());
    let c0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("c").to_owned());
    let c1 = ModuleId::new(UPGRADE_ACCOUNT, ident_str!("c").to_owned());

    let c0_modules = get_relinker_tests_modules_with_deps("c_v0", []).unwrap();
    let c1_modules = get_relinker_tests_modules_with_deps("c_v1", []).unwrap();
    let b0_modules = get_relinker_tests_modules_with_deps("b_v0", ["c_v0"]).unwrap();
    let a0_modules = get_relinker_tests_modules_with_deps("a_v0", ["b_v0", "c_v1"]).unwrap();

    // Publish the first version of C, and B which is published depending on it.
    adapter.publish_modules(c0_modules);
    adapter.publish_modules(b0_modules);

    assert_eq!(
        vec![MoveValue::U64(42 + 1)],
        adapter.call_function_with_return(&b0, ident_str!("b")),
    );

    let mut adapter = adapter.relink(
        UPGRADE_ACCOUNT,
        /* linkage */ BTreeMap::from_iter([(c0.clone(), c1.clone())]),
        /* type origin */
        BTreeMap::from_iter([
            ((c0.clone(), ident_str!("S").to_owned()), c0.clone()),
            ((c1.clone(), ident_str!("R").to_owned()), c1.clone()),
        ]),
    );

    // Publish the next version of C, and then A which depends on the new version of C, but also B.
    // B will be relinked to use C when executed in the adapter relinking against A.
    adapter.publish_modules(c1_modules);
    adapter.publish_modules(a0_modules);

    assert_eq!(
        vec![MoveValue::U64(44 + 43 + 1)],
        adapter.call_function_with_return(&a0, ident_str!("a")),
    );
}

#[test]
fn relink_publish_err() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let c0_modules = get_relinker_tests_modules_with_deps("c_v0", []).unwrap();
    let b1_modules = get_relinker_tests_modules_with_deps("b_v1", ["c_v1"]).unwrap();

    // B was built against the later version of C but published against the earlier version,
    // which should fail because a function is missing.
    adapter.publish_modules(c0_modules);
    adapter.publish_modules_with_error(b1_modules);
}

#[test]
fn relink_load_err() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let b0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("b").to_owned());
    let b1 = ModuleId::new(UPGRADE_ACCOUNT, ident_str!("b").to_owned());
    let c0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("c").to_owned());
    let c1 = ModuleId::new(UPGRADE_ACCOUNT, ident_str!("c").to_owned());

    let c0_modules = get_relinker_tests_modules_with_deps("c_v0", []).unwrap();
    let c1_modules = get_relinker_tests_modules_with_deps("c_v1", []).unwrap();
    let b0_modules = get_relinker_tests_modules_with_deps("b_v0", ["c_v0"]).unwrap();
    let b1_modules = get_relinker_tests_modules_with_deps("b_v1", ["c_v1"]).unwrap();

    // B v0 works with C v0
    adapter.publish_modules(c0_modules);
    adapter.publish_modules(b0_modules);

    assert_eq!(
        vec![MoveValue::U64(42 + 1)],
        adapter.call_function_with_return(&b0, ident_str!("b")),
    );

    let mut adapter = adapter.relink(
        UPGRADE_ACCOUNT,
        /* linkage */
        BTreeMap::from_iter([(b0.clone(), b1.clone()), (c0.clone(), c1.clone())]),
        /* type origin */
        BTreeMap::from_iter([
            ((c0.clone(), ident_str!("S").to_owned()), c0.clone()),
            ((c0.clone(), ident_str!("R").to_owned()), c1.clone()),
        ]),
    );

    // B v1 works with C v1
    adapter.publish_modules(c1_modules);
    adapter.publish_modules(b1_modules);

    assert_eq!(
        vec![MoveValue::U64(44 * 43)],
        adapter.call_function_with_return(&b0, ident_str!("b")),
    );

    let adapter = adapter.relink(
        UPGRADE_ACCOUNT,
        /* linkage */
        BTreeMap::from_iter([(b0.clone(), b1.clone()), (c0.clone(), c0.clone())]),
        /* type origin */
        BTreeMap::from_iter([
            ((b0.clone(), ident_str!("S").to_owned()), b1.clone()),
            ((c0.clone(), ident_str!("S").to_owned()), c0.clone()),
        ]),
    );

    // But B v1 *does not* work with C v0
    adapter.call_function_with_error(&b0, ident_str!("b0"));
}

#[test]
fn relink_type_identity() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let b0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("b").to_owned());
    let c0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("c").to_owned());
    let b1 = ModuleId::new(UPGRADE_ACCOUNT, ident_str!("b").to_owned());
    let c1 = ModuleId::new(UPGRADE_ACCOUNT, ident_str!("c").to_owned());
    let c0_modules = get_relinker_tests_modules_with_deps("c_v0", []).unwrap();
    let c1_modules = get_relinker_tests_modules_with_deps("c_v1", []).unwrap();
    let b1_modules = get_relinker_tests_modules_with_deps("b_v1", ["c_v1"]).unwrap();

    adapter.publish_modules(c0_modules);
    let c0_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());

    let mut adapter = adapter.relink(
        UPGRADE_ACCOUNT,
        /* linkage */
        BTreeMap::from_iter([(b0.clone(), b1.clone()), (c0.clone(), c1.clone())]),
        /* type origin */
        BTreeMap::from_iter([
            ((b0.clone(), ident_str!("S").to_owned()), b1.clone()),
            ((c0.clone(), ident_str!("S").to_owned()), c0.clone()),
            ((c0.clone(), ident_str!("R").to_owned()), c1.clone()),
        ]),
    );

    adapter.publish_modules(c1_modules);
    adapter.publish_modules(b1_modules);

    let c1_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());
    let b1_s = adapter.load_type(&TypeTag::from_str("0x2::b::S").unwrap());

    assert_eq!(c0_s, c1_s);
    assert_ne!(c1_s, b1_s);
}

#[test]
fn relink_defining_module_successive() {
    // This test simulates building up a sequence of upgraded packages over a number of publishes
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let c0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("c").to_owned());
    let c1 = ModuleId::new(UPGRADE_ACCOUNT, ident_str!("c").to_owned());
    let c2 = ModuleId::new(UPGRADE_ACCOUNT_2, ident_str!("c").to_owned());

    let c0_modules = get_relinker_tests_modules_with_deps("c_v0", []).unwrap();
    let c1_modules = get_relinker_tests_modules_with_deps("c_v1", []).unwrap();
    let c2_modules = get_relinker_tests_modules_with_deps("c_v2", []).unwrap();

    adapter.publish_modules(c0_modules);
    let c0_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());

    let mut adapter = adapter.relink(
        UPGRADE_ACCOUNT,
        /* linkage */ BTreeMap::from_iter([(c0.clone(), c1.clone())]),
        /* type origin */
        BTreeMap::from_iter([
            ((c0.clone(), ident_str!("S").to_owned()), c0.clone()),
            ((c0.clone(), ident_str!("R").to_owned()), c1.clone()),
        ]),
    );

    adapter.publish_modules(c1_modules);
    let c1_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());
    let c1_r = adapter.load_type(&TypeTag::from_str("0x2::c::R").unwrap());

    let mut adapter = adapter.relink(
        UPGRADE_ACCOUNT_2,
        /* linkage */ BTreeMap::from_iter([(c0.clone(), c2.clone())]),
        /* type origin */
        BTreeMap::from_iter([
            ((c0.clone(), ident_str!("S").to_owned()), c0.clone()),
            ((c0.clone(), ident_str!("R").to_owned()), c1.clone()),
            ((c0.clone(), ident_str!("Q").to_owned()), c2.clone()),
        ]),
    );

    adapter.publish_modules(c2_modules);
    let c2_s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());
    let c2_r = adapter.load_type(&TypeTag::from_str("0x2::c::R").unwrap());
    let c2_q = adapter.load_type(&TypeTag::from_str("0x2::c::Q").unwrap());

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

    let c0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("c").to_owned());
    let c1 = ModuleId::new(UPGRADE_ACCOUNT, ident_str!("c").to_owned());
    let c2 = ModuleId::new(UPGRADE_ACCOUNT_2, ident_str!("c").to_owned());

    let c2_modules = get_relinker_tests_modules_with_deps("c_v2", []).unwrap();

    let mut adapter = Adapter::new(data_store).relink(
        UPGRADE_ACCOUNT_2,
        /* linkage */ BTreeMap::from_iter([(c0.clone(), c2.clone())]),
        /* type origin */
        BTreeMap::from_iter([
            ((c0.clone(), ident_str!("S").to_owned()), c0.clone()),
            ((c0.clone(), ident_str!("R").to_owned()), c1.clone()),
            ((c0.clone(), ident_str!("Q").to_owned()), c2.clone()),
        ]),
    );

    adapter.publish_modules(c2_modules);
    let s = adapter.load_type(&TypeTag::from_str("0x2::c::S").unwrap());
    let r = adapter.load_type(&TypeTag::from_str("0x2::c::R").unwrap());
    let q = adapter.load_type(&TypeTag::from_str("0x2::c::Q").unwrap());

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

#[test]
fn relink_defining_module_cleanup() {
    // If loading fails for a module that pulls in a module that was defined at an earlier version
    // of the package, roll-back should occur cleanly.
    let data_store = InMemoryStorage::new();

    let c0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("c").to_owned());
    let b0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("b").to_owned());
    let b1 = ModuleId::new(UPGRADE_ACCOUNT, ident_str!("b").to_owned());

    let mut adapter = Adapter::new(data_store).relink(
        UPGRADE_ACCOUNT,
        /* linkage */
        BTreeMap::from_iter([(b0.clone(), b1.clone()), (c0.clone(), c0.clone())]),
        /* type origin */
        BTreeMap::from_iter([
            ((c0.clone(), ident_str!("S").to_owned()), c0.clone()),
            ((b0.clone(), ident_str!("S").to_owned()), b1.clone()),
        ]),
    );

    let c0_modules = get_relinker_tests_modules_with_deps("c_v0", []).unwrap();
    let b1_modules = get_relinker_tests_modules_with_deps("b_v1", ["c_v1"]).unwrap();

    // B was built against the later version of C but published against the earlier version,
    // which should fail because a function is missing.
    adapter.publish_modules(c0_modules);

    // Somehow dependency verification fails, and the publish succeeds.
    fail::cfg("verifier-failpoint-4", "100%return").unwrap();
    adapter.publish_modules(b1_modules);

    // This call should fail to load the module and rollback cleanly
    adapter.call_function_with_error(&b0, ident_str!("b"));

    // Restore old behavior of failpoint
    fail::cfg("verifier-failpoint-4", "off").unwrap();
}

#[test]
fn publish_bundle_and_load() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let a0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("a").to_owned());
    let c1_modules = get_relinker_tests_modules_with_deps("c_v1", []).unwrap();
    let b0_modules = get_relinker_tests_modules_with_deps("b_v0", ["c_v0"]).unwrap();
    let a0_modules = get_relinker_tests_modules_with_deps("a_v0", ["b_v0", "c_v1"]).unwrap();

    let mut modules = vec![];
    modules.extend(c1_modules);
    modules.extend(b0_modules);
    modules.extend(a0_modules);

    // Publish all the modules together
    adapter.publish_module_bundle(modules);

    assert_eq!(
        vec![MoveValue::U64(44 + 43 + 1)],
        adapter.call_function_with_return(&a0, ident_str!("a")),
    );
}

#[test]
fn publish_bundle_with_err_retry() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let a0 = ModuleId::new(DEFAULT_ACCOUNT, ident_str!("a").to_owned());
    let c0_modules = get_relinker_tests_modules_with_deps("c_v0", []).unwrap();
    let c1_modules = get_relinker_tests_modules_with_deps("c_v1", []).unwrap();
    let b0_modules = get_relinker_tests_modules_with_deps("b_v0", ["c_v0"]).unwrap();
    let a0_modules = get_relinker_tests_modules_with_deps("a_v0", ["b_v0", "c_v1"]).unwrap();

    let mut modules = vec![];
    modules.extend(c0_modules);
    modules.extend(b0_modules.clone());
    modules.extend(a0_modules.clone());

    // Publishing the bundle should fail, because `a0` should not link with `c0`.
    adapter.publish_module_bundle_with_error(modules);

    let mut modules = vec![];
    modules.extend(c1_modules);
    modules.extend(b0_modules);
    modules.extend(a0_modules);

    // Try again and everything should publish successfully (in particular, the failed publish
    // will not leave behind modules in the loader).
    adapter.publish_module_bundle(modules);

    assert_eq!(
        vec![MoveValue::U64(44 + 43 + 1)],
        adapter.call_function_with_return(&a0, ident_str!("a")),
    );
}

#[test]
fn deep_dependency_list_err_0() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 350u64;
    dependency_chain(1, max, &mut modules);
    adapter.publish_modules(modules);

    let mut adapter = adapter.fresh();
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(name, deps);
    adapter.publish_modules_with_error(vec![module]);
}

#[test]
fn deep_dependency_list_err_1() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 101u64;
    dependency_chain(1, max, &mut modules);
    adapter.publish_modules(modules);

    let mut adapter = adapter.fresh();
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(name, deps);
    adapter.publish_modules_with_error(vec![module]);
}

#[test]
fn deep_dependency_list_ok_0() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 100u64;
    dependency_chain(1, max, &mut modules);
    adapter.publish_modules(modules);

    let mut adapter = adapter.fresh();
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(name, deps);
    adapter.publish_modules(vec![module]);
}

#[test]
fn deep_dependency_list_ok_1() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 30u64;
    dependency_chain(1, max, &mut modules);
    adapter.publish_modules(modules);

    let mut adapter = adapter.fresh();
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(name, deps);
    adapter.publish_modules(vec![module]);
}

#[test]
fn deep_dependency_tree_err_0() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a tree of dependencies
    let width = 5u64;
    let height = 101u64;
    dependency_tree(width, height, &mut modules);
    adapter.publish_modules(modules);

    // use one of the module in the tree
    let mut adapter = adapter.fresh();
    let name = "ASome".to_string();
    let dep_name = format!("A_{}_{}", height - 1, width - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(name, deps);
    adapter.publish_modules_with_error(vec![module]);
}

#[test]
fn deep_dependency_tree_err_1() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a tree of dependencies
    let width = 3u64;
    let height = 350u64;
    dependency_tree(width, height, &mut modules);
    adapter.publish_modules(modules);

    // use one of the module in the tree
    let mut adapter = adapter.fresh();
    let name = "ASome".to_string();
    let dep_name = format!("A_{}_{}", height - 1, width - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(name, deps);
    adapter.publish_modules_with_error(vec![module]);
}

#[test]
fn deep_dependency_tree_ok_0() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a tree of dependencies
    let width = 10u64;
    let height = 20u64;
    dependency_tree(width, height, &mut modules);
    adapter.publish_modules(modules);

    // use one of the module in the tree
    let mut adapter = adapter.fresh();
    let name = "ASome".to_string();
    let dep_name = format!("A_{}_{}", height - 1, width - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(name, deps);
    adapter.publish_modules(vec![module]);
}

#[test]
fn deep_dependency_tree_ok_1() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a tree of dependencies
    let width = 3u64;
    let height = 100u64;
    dependency_tree(width, height, &mut modules);
    adapter.publish_modules(modules);

    // use one of the module in the tree
    let mut adapter = adapter.fresh();
    let name = "ASome".to_string();
    let dep_name = format!("A_{}_{}", height - 1, width - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_dependencies(name, deps);
    adapter.publish_modules(vec![module]);
}

#[test]
fn deep_friend_list_ok_0() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 100u64;
    friend_chain(1, max, &mut modules);
    adapter.publish_modules(modules);

    let mut adapter = adapter.fresh();
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_friends(name, deps);
    adapter.publish_modules(vec![module]);
}

#[test]
fn deep_friend_list_ok_1() {
    let data_store = InMemoryStorage::new();
    let mut adapter = Adapter::new(data_store);

    let mut modules = vec![];

    // create a chain of dependencies
    let max = 30u64;
    friend_chain(1, max, &mut modules);
    adapter.publish_modules(modules);

    let mut adapter = adapter.fresh();
    let name = format!("A{}", max);
    let dep_name = format!("A{}", max - 1);
    let deps = vec![dep_name];
    let module = empty_module_with_friends(name, deps);
    adapter.publish_modules(vec![module]);
}

fn leaf_module(name: &str) -> CompiledModule {
    let mut module = empty_module();
    module.identifiers[0] = Identifier::new(name).unwrap();
    module.address_identifiers[0] = DEFAULT_ACCOUNT;
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
        let module = empty_module_with_dependencies(name, deps);
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
            let module = empty_module_with_dependencies(name.clone(), deps.clone());
            new_deps.push(name);
            modules.push(module);
        }
        deps = new_deps;
    }
}

// Create a module that uses (depends on) the list of given modules
fn empty_module_with_dependencies(name: String, deps: Vec<String>) -> CompiledModule {
    let mut module = empty_module();
    module.address_identifiers[0] = DEFAULT_ACCOUNT;
    module.identifiers[0] = Identifier::new(name).unwrap();
    for dep in deps {
        module.identifiers.push(Identifier::new(dep).unwrap());
        module.module_handles.push(ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex((module.identifiers.len() - 1) as TableIndex),
        });
    }
    module
}

// Create a list of friends modules
fn friend_chain(start: u64, end: u64, modules: &mut Vec<CompiledModule>) {
    let module = leaf_module("A0");
    modules.push(module);

    for i in start..end {
        let name = format!("A{}", i);
        let dep_name = format!("A{}", i - 1);
        let deps = vec![dep_name];
        let module = empty_module_with_friends(name, deps);
        modules.push(module);
    }
}

// Create a module that uses (friends on) the list of given modules
fn empty_module_with_friends(name: String, deps: Vec<String>) -> CompiledModule {
    let mut module = empty_module();
    module.address_identifiers[0] = DEFAULT_ACCOUNT;
    module.identifiers[0] = Identifier::new(name).unwrap();
    for dep in deps {
        module.identifiers.push(Identifier::new(dep).unwrap());
        module.friend_decls.push(ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex((module.identifiers.len() - 1) as TableIndex),
        });
    }
    module
}
