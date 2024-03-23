// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Load all packages in memory and build a `GlobalEnv`.
/// Perform all the necessary checks to build the environment.
use crate::{
    model::{
        compiled_module_util::{
            get_package_from_function_handle, get_package_from_struct_def,
            get_package_from_struct_handle, module_function_name_from_def,
            module_function_name_from_handle, module_struct_name_from_def,
            module_struct_name_from_handle,
        },
        global_env::GlobalEnv,
        move_model::{
            Bytecode, Code, Constant, Field, FieldRef, Function, FunctionIndex, IdentifierIndex,
            Module, ModuleIndex, Package, PackageIndex, Struct, StructIndex, Type,
        },
    },
    DEFAULT_CAPACITY, FRAMEWORK,
};
use move_binary_format::access::ModuleAccess;
use move_binary_format::{
    file_format::{
        Bytecode as MoveBytecode, ConstantPoolIndex, FunctionDefinitionIndex, FunctionHandleIndex,
        SignatureToken, StructDefinitionIndex, StructFieldInformation, StructHandleIndex,
    },
    CompiledModule,
};
use std::collections::{BTreeMap, BTreeSet};
use sui_types::{base_types::ObjectID, move_package::MovePackage};

/// Single entry point into this module, builds a `GlobalEnv` from a collection of `MovePackage`'s.
pub fn build_environment(packages: Vec<MovePackage>) -> GlobalEnv {
    let mut identifier_map = IdentifierMap::new();

    let (mut packages, package_map, framework) = load_packages(packages);
    let (mut modules, module_map) = load_modules(&mut identifier_map, &mut packages);
    load_versions(&mut packages, &package_map, &modules);
    let (mut structs, struct_map) = load_structs(&mut identifier_map, &mut modules, &packages);
    module_dependencies(&mut packages, &mut modules, &structs, &identifier_map);
    package_dependencies(&mut packages, &modules, &package_map);

    // load fields and constants
    let type_builder = TypeBuilder {
        packages,
        struct_map,
    };
    load_fields(&mut structs, &mut identifier_map, &type_builder, &modules);
    load_constants(&type_builder, &mut modules);

    // load functions and code
    let (mut functions, function_map) =
        load_functions(&mut identifier_map, &mut modules, &type_builder);
    load_code(&mut functions, &type_builder, &modules, &function_map);

    // build `GlobalEnv`
    let IdentifierMap {
        identifiers,
        identifier_map,
    } = identifier_map;
    let TypeBuilder {
        packages,
        struct_map,
    } = type_builder;

    // build call graphs
    let (call_graph, reverse_call_graph) = build_call_graphs(&functions);

    // build and return the environment
    GlobalEnv {
        packages,
        modules,
        functions,
        structs,
        identifiers,
        package_map,
        module_map,
        function_map,
        struct_map,
        identifier_map,
        signatures: BTreeMap::new(),
        callers: call_graph,
        callees: reverse_call_graph,
        framework,
    }
}

// Intern table for identifiers.
// Used temporarily while loading the environment.
// At the end of loading is destructured and `identifiers` and `identifier_map` are moved into the `GlobalEnv`.
#[derive(Debug)]
struct IdentifierMap {
    identifiers: Vec<String>,
    identifier_map: BTreeMap<String, IdentifierIndex>,
}

impl IdentifierMap {
    fn new() -> Self {
        Self {
            identifiers: Vec::with_capacity(DEFAULT_CAPACITY),
            identifier_map: BTreeMap::new(),
        }
    }

    // Itern an identifier and return its index in the intern table.
    fn get_identifier_idx(&mut self, ident: &String) -> IdentifierIndex {
        if let Some(idx) = self.identifier_map.get(ident) {
            return *idx;
        }
        let idx = self.identifiers.len();
        self.identifiers.push(ident.clone());
        self.identifier_map.insert(ident.clone(), idx);
        idx
    }

    // Get an identifier from its index.
    fn get_identifier(&self, idx: IdentifierIndex) -> &str {
        self.identifiers[idx].as_str()
    }
}

// Build types from signatures.
// All signatures are expanded to vector of types.
// Used temporarily while loading the environment.
// At the end of loading is destructured and `packages` and `struct_map` are moved into the `GlobalEnv`.
struct TypeBuilder {
    packages: Vec<Package>,
    struct_map: BTreeMap<String, StructIndex>,
}

impl TypeBuilder {
    fn make_type(&self, module: &Module, type_: &SignatureToken) -> Type {
        match type_ {
            SignatureToken::Bool => Type::Bool,
            SignatureToken::U8 => Type::U8,
            SignatureToken::U16 => Type::U16,
            SignatureToken::U32 => Type::U32,
            SignatureToken::U64 => Type::U64,
            SignatureToken::U128 => Type::U128,
            SignatureToken::U256 => Type::U256,
            SignatureToken::Address => Type::Address,
            SignatureToken::Vector(inner) => Type::Vector(Box::new(self.make_type(module, inner))),
            SignatureToken::Struct(struct_handle_idx) => {
                let idx = self
                    .get_struct_idx(module, *struct_handle_idx)
                    .unwrap_or_else(|| {
                        panic!(
                            "\nFailure getting struct index for struct handle {:?}\nPackage {}, module id {}\n",
                            struct_handle_idx,
                            self.packages[module.package].id,
                            module.module_id,
                        )
                    });
                Type::Struct(idx)
            }
            SignatureToken::StructInstantiation(struct_handle_idx, type_arguments) => {
                let idx = self
                    .get_struct_idx(module, *struct_handle_idx)
                    .unwrap_or_else(|| {
                        panic!(
                            "\nFailure getting struct index for struct handle {:?}\nPackage {}, module id {}\n",
                            struct_handle_idx,
                            self.packages[module.package].id,
                            module.module_id,
                        )
                    });
                let type_arguments = type_arguments
                    .iter()
                    .map(|type_| self.make_type(module, type_))
                    .collect::<Vec<_>>();
                Type::StructInstantiation(idx, type_arguments)
            }
            SignatureToken::Reference(inner) => {
                Type::Reference(Box::new(self.make_type(module, inner)))
            }
            SignatureToken::MutableReference(inner) => {
                Type::MutableReference(Box::new(self.make_type(module, inner)))
            }
            SignatureToken::TypeParameter(idx) => Type::TypeParameter(*idx),
            _ => panic!("Invalid type found: {:?}", type_),
        }
    }

    fn get_struct_idx(
        &self,
        module: &Module,
        struct_handle_idx: StructHandleIndex,
    ) -> Option<StructIndex> {
        let compiled_module = module.module.as_ref().unwrap();
        let (module_name, struct_name) =
            module_struct_name_from_handle(compiled_module, struct_handle_idx);
        let package_id = get_package_from_struct_handle(compiled_module, struct_handle_idx);
        let package = &self.packages[module.package];
        let key = (module_name.to_string(), struct_name.to_string());
        let package_id = match package.type_origin.get(&key) {
            None => ObjectID::from(package_id),
            Some(package_id) => *package_id,
        };
        let package_id = match package
            .package
            .as_ref()
            .unwrap()
            .linkage_table()
            .get(&package_id)
        {
            None => package_id,
            Some(upgrade_info) => upgrade_info.upgraded_id,
        };
        let struct_key = format!("{}::{}::{}", package_id, module_name, struct_name);
        self.struct_map.get(&struct_key).copied()
    }
}

// Load all packages and return a vector of `Package`'s and a map of package id to `PackageIndex`'s.
fn load_packages(
    packages: Vec<MovePackage>,
) -> (
    Vec<Package>,                     // all packages
    BTreeMap<ObjectID, PackageIndex>, // map from package id to index in the vector of packages
    BTreeMap<PackageIndex, ObjectID>, // map from package index (position in the vector) to package id
) {
    let mut framework = BTreeMap::new();
    // global package vector; all packages provided
    let mut packages = packages
        .into_iter()
        .enumerate()
        .map(|(self_idx, package)| {
            if FRAMEWORK.contains(&package.id()) {
                framework.insert(self_idx, package.id());
            }
            let type_origin = package.type_origin_map();
            let version = package.version().value();
            Package {
                self_idx,
                id: package.id(),
                package: Some(package),
                version,
                type_origin,
                type_dependencies: BTreeSet::new(),
                dependencies: BTreeMap::new(),
                direct_dependencies: BTreeSet::new(),
                versions: vec![],
                root_version: None,
                modules: vec![],
            }
        })
        .collect::<Vec<_>>();
    // package map; from package id to package index (position in the vector)
    let package_map = packages
        .iter()
        .enumerate()
        .map(|(idx, package)| (package.id, idx))
        .collect::<BTreeMap<_, _>>();

    // fix up dependencies, make a map from package index to package index (positions in the vector)
    packages.iter_mut().for_each(|package| {
        let move_package = package.package.as_ref().unwrap();
        package.dependencies = move_package
            .linkage_table()
            .iter()
            .map(|(base, version)| {
                let base_id = package_map[base];
                let version_id = package_map[&version.upgraded_id];
                (base_id, version_id)
            })
            .collect::<BTreeMap<_, _>>();
    });

    (packages, package_map, framework)
}

// Load all modules and return a vector of `Module`'s and a map of module name to `ModuleIndex`'s.
// Module keys are built from the package id and the module name: <package_id>::<module_name>.
// Package id and address of the `CompiledModule` will be different when versioning packages.
fn load_modules(
    identifier_map: &mut IdentifierMap,
    packages: &mut [Package],
) -> (Vec<Module>, BTreeMap<String, ModuleIndex>) {
    // global module vector
    let mut modules = packages
        .iter()
        .enumerate()
        .flat_map(|(pkg_idx, package)| {
            let package_id = package.id;
            let move_package = package.package.as_ref().unwrap();
            let modules: Vec<(&str, CompiledModule)> = move_package
                .serialized_module_map()
                .iter()
                .map(|(name, bytes)| {
                    (
                        name.as_str(),
                        CompiledModule::deserialize_with_defaults(bytes).unwrap_or_else(|err| {
                            panic!(
                                "Failure deserializing module {} in package {}: {:?}",
                                name, package_id, err,
                            )
                        }),
                    )
                })
                .collect::<Vec<_>>();
            modules
                .into_iter()
                .map(|(name, module)| {
                    let module_id = module.self_id();
                    let module_name = module_id.name().as_str();
                    assert_eq!(
                        name, module_name,
                        "Mismatch in package {}: module name {} and name for ModuleId {}",
                        package_id, name, module_id,
                    );
                    let name = identifier_map.get_identifier_idx(&module_name.to_string());
                    Module {
                        self_idx: 0, // initialized later
                        package: pkg_idx,
                        name,
                        module_id,
                        type_dependencies: BTreeSet::new(),
                        dependencies: BTreeSet::new(),
                        structs: vec![],
                        functions: vec![],
                        constants: vec![],
                        module: Some(module),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    // module map; from module key (package::module_name) to module index (position in the vector)
    let module_map = modules
        .iter_mut()
        .enumerate()
        .map(|(idx, module)| {
            // update module and packages
            module.self_idx = idx;
            let package = &mut packages[module.package];
            package.modules.push(idx);
            let package_id = package.id;
            let module_name = identifier_map.get_identifier(module.name);
            assert_eq!(
                module_name,
                module.module_id.name().as_str(),
                "Mismatch in package {}: module name {} and name in ModuleId {}",
                package_id,
                module_name,
                module.module_id,
            );
            let key = format!("{}::{}", package_id, module_name);
            (key, idx)
        })
        .collect::<BTreeMap<_, _>>();

    (modules, module_map)
}

/// Load module dependencies.
fn module_dependencies(
    packages: &mut [Package],
    modules: &mut [Module],
    structs: &[Struct],
    identifier_map: &IdentifierMap,
) {
    modules.iter_mut().for_each(|module| {
        let package = &packages[module.package];

        //
        // create dependencies for each module
        //

        // each struct must be defined either in this package or
        // in a previous version of this package.
        // We collect previous packages in a `type_dependencies` set.
        let type_deps: BTreeSet<_> = module
            .structs
            .iter()
            .map(|struct_idx| {
                let struct_ = &structs[*struct_idx];

                let struct_name = identifier_map.get_identifier(struct_.name);
                let module_name = identifier_map.get_identifier(module.name);
                let type_origin_key = (module_name.to_string(), struct_name.to_string());
                *package
                    .type_origin
                    .get(&type_origin_key)
                    .unwrap_or_else(|| {
                        panic!(
                            "Unable to find type origin for struct {} in package {} [{}], module {}",
                            struct_name,
                            package.id,
                            package.version,
                            module.module_id,
                        )
                    })
            })
            .filter(|dep| *dep != package.id)
            .collect();
        module.type_dependencies.extend(&type_deps);

        // every other module handle (except self) is considered a dependency
        let compiled_module = module.module.as_ref().unwrap();
        let deps: BTreeSet<_> = compiled_module
            .module_handles
            .iter()
            .filter(|module_handle| {
                *module_handle != compiled_module.self_handle()
            })
            .map(|module_handle| {
                ObjectID::from_address(
                    compiled_module.address_identifiers[module_handle.address.0 as usize],
                )
            })
            .filter(|dep| !type_deps.contains(dep))
            .collect();
        assert_eq!(
            module.type_dependencies.intersection(&deps).count(), 0,
            "Module 0x{} in package {} [{}] has type origins and dependencies in common",
            module.module_id, package.id, package.version,
        );
        module.dependencies.extend(&deps);
    });
}

// Break up dependencies and load type dependencies (origin table)
fn package_dependencies(
    packages: &mut [Package],
    modules: &[Module],
    package_map: &BTreeMap<ObjectID, PackageIndex>,
) {
    packages.iter_mut().for_each(|package| {
        package.modules.iter().for_each(|module_idx| {
            let module = &modules[*module_idx];
            module.type_dependencies.iter().for_each(|module_dep| {
                let package_index = *package_map.get(module_dep).unwrap_or_else(|| {
                    panic!("Unable to find package index for package {}", module_dep)
                });
                package.type_dependencies.insert(package_index);
            });
            module.dependencies.iter().for_each(|module_dep| {
                let package_index = *package_map.get(module_dep).unwrap_or_else(|| {
                    panic!("Unable to find package index for package {}", module_dep)
                });
                if package_index != package.self_idx && Some(package_index) != package.root_version
                {
                    let dep_idx = package.dependencies.get(&package_index).unwrap_or_else(|| {
                        panic!(
                            "Unable to find package {} in dependencies of package {}",
                            module_dep, package.id,
                        )
                    });
                    package.direct_dependencies.insert(*dep_idx);
                }
            });
        });
    });
}

// Discover package versions and load them in the root package.
// Versioned packages will contain the `PackageIndex` of the root.
fn load_versions(
    packages: &mut [Package],
    package_map: &BTreeMap<ObjectID, PackageIndex>,
    modules: &[Module],
) {
    let mut versions = BTreeMap::new();
    packages.iter().for_each(|package| {
        check_package(package, modules);
        let package_idx = package.self_idx;
        let package_id = package.id;
        let version = package.version;
        let module = &modules[package.modules[0]];
        let origin = ObjectID::from(*module.module_id.address());
        if origin != package_id {
            let value = versions.entry(origin).or_insert(vec![]);
            value.push(package_idx);
        } else {
            if !FRAMEWORK.contains(&package_id) {
                assert_eq!(version, 1, "package {} is not version 1", package_id);
            }
            versions.entry(package_id).or_insert(vec![]);
        }
    });
    versions.iter_mut().for_each(|(_, v)| {
        v.sort_by(|pkg1, pkg2| packages[*pkg1].version.cmp(&packages[*pkg2].version))
    });
    versions.iter().for_each(|(id, versions)| {
        let root_idx = package_map[id];
        versions.iter().for_each(|version| {
            packages[*version].root_version = Some(root_idx);
        });
    });
    versions.into_iter().for_each(|(id, versions)| {
        let package = &mut packages[package_map[&id]];
        package.versions = versions;
    });
    verify_versions(packages);
}

// Load all `Struct`s in GlobalEnv.
// Structs are keyed off the package they are in: <package_id>::<module_name>::<struct_name>
// and not via the address of the Module (ModuleID).
// That is not an issue with the first version of a package but with new version of packages
// the address of the module stays the same but the package id changes.
fn load_structs(
    identifier_map: &mut IdentifierMap,
    modules: &mut [Module],
    packages: &[Package],
) -> (Vec<Struct>, BTreeMap<String, StructIndex>) {
    // structs
    let mut structs = modules
        .iter()
        .enumerate()
        .flat_map(|(idx, module)| {
            let compiled_module = module.module.as_ref().unwrap();
            assert_eq!(
                identifier_map.get_identifier(module.name),
                module.module_id.name().as_str(),
                "Mismatch in module name: env name {}, handle name {}",
                identifier_map.get_identifier(module.name),
                module.module_id.name().as_str(),
            );
            compiled_module
                .struct_defs
                .iter()
                .enumerate()
                .map(|(def_idx, struct_def)| {
                    let struct_handle =
                        &compiled_module.struct_handles[struct_def.struct_handle.0 as usize];
                    let abilities = struct_handle.abilities;
                    let type_parameters = struct_handle.type_parameters.clone();
                    let struct_name = module.module.as_ref().unwrap().identifiers
                        [struct_handle.name.0 as usize]
                        .to_string();
                    let name = identifier_map.get_identifier_idx(&struct_name);
                    Struct {
                        self_idx: 0, // initialized later
                        package: module.package,
                        module: idx,
                        name,
                        def_idx: StructDefinitionIndex(def_idx as u16),
                        abilities,
                        type_parameters,
                        fields: vec![],
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    // struct map
    let struct_map = structs
        .iter_mut()
        .enumerate()
        .map(|(idx, struct_)| {
            struct_.self_idx = idx;
            modules[struct_.module].structs.push(idx);
            let package_id = packages[struct_.package].id;
            let module = &modules[struct_.module];
            let compiled_module = module.module.as_ref().unwrap();
            let (mh_name, st_name) = module_struct_name_from_def(compiled_module, struct_.def_idx);
            let module_name = identifier_map.get_identifier(module.name);
            let struct_name = identifier_map.get_identifier(struct_.name);
            assert_eq!(
                module_name, mh_name,
                "Mismatch in module name: env name {}, module handle name {}",
                module_name, mh_name,
            );
            assert_eq!(
                struct_name, st_name,
                "Mismatch in struct name: env name {}, struct handle name {}",
                struct_name, st_name,
            );
            let key = format!("{}::{}::{}", package_id, module_name, struct_name);
            (key, idx)
        })
        .collect::<BTreeMap<_, _>>();

    (structs, struct_map)
}

// Load all fields in `Struct`s.
fn load_fields(
    structs: &mut [Struct],
    identifier_map: &mut IdentifierMap,
    type_builder: &TypeBuilder,
    modules: &[Module],
) {
    structs.iter_mut().for_each(|struct_| {
        let module = &modules[struct_.module];
        let compiled_module = module.module.as_ref().unwrap();
        let struct_def = &compiled_module.struct_defs[struct_.def_idx.0 as usize];
        let fields = if let StructFieldInformation::Declared(fields) = &struct_def.field_information
        {
            fields
                .iter()
                .map(|field| {
                    let name = compiled_module.identifiers[field.name.0 as usize].to_string();
                    Field {
                        name: identifier_map.get_identifier_idx(&name),
                        type_: type_builder.make_type(module, &field.signature.0),
                    }
                })
                .collect::<Vec<_>>()
        } else {
            panic!(
                "Found native field in module {} in package {}",
                compiled_module.self_id(),
                module.package,
            )
        };
        struct_.fields = fields;
    });
}

// Load all `Function`s in GlobalEnv.
// Functions are keyed off the package they are in: <package_id>::<module_name>::<function_name>
// and not via the address of the Module (ModuleID).
// That is not an issue with the first version of a package but with new version of packages
// the address of the module stays the same but the package id changes.
fn load_functions(
    identifier_map: &mut IdentifierMap,
    modules: &mut [Module],
    type_builder: &TypeBuilder,
) -> (Vec<Function>, BTreeMap<String, FunctionIndex>) {
    // functions
    let mut functions = modules
        .iter()
        .flat_map(|module| {
            let compiled_module = module.module.as_ref().unwrap();
            compiled_module
                .function_defs
                .iter()
                .enumerate()
                .map(|(def_idx, func_def)| {
                    let func_handle =
                        &compiled_module.function_handles[func_def.function.0 as usize];

                    let visibility = func_def.visibility;
                    let is_entry = func_def.is_entry;
                    let type_parameters = func_handle.type_parameters.clone();

                    let params = &compiled_module.signatures[func_handle.parameters.0 as usize];
                    let parameters = params
                        .0
                        .iter()
                        .map(|type_| type_builder.make_type(module, type_))
                        .collect::<Vec<_>>();

                    let rets = &compiled_module.signatures[func_handle.return_.0 as usize];
                    let returns = rets
                        .0
                        .iter()
                        .map(|type_| type_builder.make_type(module, type_))
                        .collect::<Vec<_>>();

                    let func_name =
                        compiled_module.identifiers[func_handle.name.0 as usize].to_string();
                    let name = identifier_map.get_identifier_idx(&func_name);
                    assert_eq!(
                        identifier_map.get_identifier(name),
                        func_name,
                        "Mismatch in function name: env name {}, module handle name {}",
                        identifier_map.get_identifier(name),
                        func_name,
                    );

                    Function {
                        self_idx: 0,
                        package: module.package,
                        module: module.self_idx,
                        name,
                        def_idx: FunctionDefinitionIndex(def_idx as u16),
                        type_parameters,
                        parameters,
                        returns,
                        visibility,
                        is_entry,
                        code: None,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    // function map
    let function_map = functions
        .iter_mut()
        .enumerate()
        .map(|(idx, function)| {
            let package_id = type_builder.packages[function.package].id;
            function.self_idx = idx;
            let module = &mut modules[function.module];
            module.functions.push(idx);
            let compiled_module = module.module.as_ref().unwrap();
            let (mh_name, func_name) =
                module_function_name_from_def(compiled_module, function.def_idx);
            let module_name = identifier_map.get_identifier(module.name);
            let function_name = identifier_map.get_identifier(function.name);
            assert_eq!(
                module_name, mh_name,
                "Mismatch in module name: env name {}, module handle name {}",
                module_name, mh_name,
            );
            assert_eq!(
                function_name, func_name,
                "Mismatch in function name: env name {}, function handle name {}",
                function_name, func_name,
            );
            let key = format!("{}::{}::{}", package_id, module_name, function_name);
            (key, idx)
        })
        .collect::<BTreeMap<_, _>>();

    (functions, function_map)
}

// Load code of each `Function`.
// Instructions and types are made to point to the `GlobalEnv` indexes.
fn load_code(
    functions: &mut [Function],
    type_builder: &TypeBuilder,
    modules: &[Module],
    function_map: &BTreeMap<String, FunctionIndex>,
) {
    functions.iter_mut().for_each(|function| {
        let module = &modules[function.module];

        macro_rules! get_from_map {
            ($key:expr, $map:expr) => {
                *$map.get($key).unwrap_or_else(|| {
                    panic!(
                        "Unable to find key {}\npackage {}, module {}, function {}\n{:#?}",
                        $key,
                        type_builder.packages[module.package].id,
                        module.module_id,
                        function.def_idx,
                        module.module.as_ref().unwrap(),
                    )
                })
            };
        }

        let compiled_module = module.module.as_ref().unwrap();
        let func_def = &compiled_module.function_defs[function.def_idx.0 as usize];
        if let Some(code_unit) = func_def.code.as_ref() {
            let locals = &compiled_module.signatures[code_unit.locals.0 as usize];
            let locals = locals
                .0
                .iter()
                .map(|type_| type_builder.make_type(module, type_))
                .collect::<Vec<_>>();
            let code: Vec<Bytecode> = code_unit
                .code
                .iter()
                .map(|bytecode| match bytecode {
                    MoveBytecode::Nop => Bytecode::Nop,
                    MoveBytecode::Pop => Bytecode::Pop,
                    MoveBytecode::Ret => Bytecode::Ret,
                    MoveBytecode::BrTrue(code_offset) => Bytecode::BrTrue(*code_offset),
                    MoveBytecode::BrFalse(code_offset) => Bytecode::BrFalse(*code_offset),
                    MoveBytecode::Branch(code_offset) => Bytecode::Branch(*code_offset),
                    MoveBytecode::LdConst(idx) => Bytecode::LdConst(*idx),
                    MoveBytecode::LdTrue => Bytecode::LdTrue,
                    MoveBytecode::LdFalse => Bytecode::LdFalse,
                    MoveBytecode::LdU8(v) => Bytecode::LdU8(*v),
                    MoveBytecode::LdU16(v) => Bytecode::LdU16(*v),
                    MoveBytecode::LdU32(v) => Bytecode::LdU32(*v),
                    MoveBytecode::LdU64(v) => Bytecode::LdU64(*v),
                    MoveBytecode::LdU128(v) => Bytecode::LdU128(*v),
                    MoveBytecode::LdU256(v) => Bytecode::LdU256(*v),
                    MoveBytecode::CastU8 => Bytecode::CastU8,
                    MoveBytecode::CastU16 => Bytecode::CastU16,
                    MoveBytecode::CastU32 => Bytecode::CastU32,
                    MoveBytecode::CastU64 => Bytecode::CastU64,
                    MoveBytecode::CastU128 => Bytecode::CastU128,
                    MoveBytecode::CastU256 => Bytecode::CastU256,
                    MoveBytecode::Add => Bytecode::Add,
                    MoveBytecode::Sub => Bytecode::Sub,
                    MoveBytecode::Mul => Bytecode::Mul,
                    MoveBytecode::Mod => Bytecode::Mod,
                    MoveBytecode::Div => Bytecode::Div,
                    MoveBytecode::BitOr => Bytecode::BitOr,
                    MoveBytecode::BitAnd => Bytecode::BitAnd,
                    MoveBytecode::Xor => Bytecode::Xor,
                    MoveBytecode::Or => Bytecode::Or,
                    MoveBytecode::And => Bytecode::And,
                    MoveBytecode::Not => Bytecode::Not,
                    MoveBytecode::Eq => Bytecode::Eq,
                    MoveBytecode::Neq => Bytecode::Neq,
                    MoveBytecode::Lt => Bytecode::Lt,
                    MoveBytecode::Gt => Bytecode::Gt,
                    MoveBytecode::Le => Bytecode::Le,
                    MoveBytecode::Ge => Bytecode::Ge,
                    MoveBytecode::Shl => Bytecode::Shl,
                    MoveBytecode::Shr => Bytecode::Shr,
                    MoveBytecode::Abort => Bytecode::Abort,
                    MoveBytecode::CopyLoc(idx) => Bytecode::CopyLoc(*idx),
                    MoveBytecode::MoveLoc(idx) => Bytecode::MoveLoc(*idx),
                    MoveBytecode::StLoc(idx) => Bytecode::StLoc(*idx),
                    MoveBytecode::Call(idx) => {
                        let func_key = get_function_key_from_handle(
                            *idx,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let func_idx = get_from_map!(&func_key, function_map);
                        Bytecode::Call(func_idx)
                    }
                    MoveBytecode::CallGeneric(idx) => {
                        let func_inst = &compiled_module.function_instantiations[idx.0 as usize];
                        let func_key = get_function_key_from_handle(
                            func_inst.handle,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let func_idx = get_from_map!(&func_key, function_map);
                        let sig_idx = func_inst.type_parameters;
                        let params = &compiled_module.signatures[sig_idx.0 as usize];
                        let type_params = params
                            .0
                            .iter()
                            .map(|type_| type_builder.make_type(module, type_))
                            .collect::<Vec<_>>();
                        Bytecode::CallGeneric(func_idx, type_params)
                    }
                    MoveBytecode::Pack(idx) => {
                        let struct_key = get_struct_key_from_def(
                            *idx,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let struct_idx = get_from_map!(&struct_key, &type_builder.struct_map);
                        Bytecode::Pack(struct_idx)
                    }
                    MoveBytecode::PackGeneric(idx) => {
                        let struct_inst =
                            &compiled_module.struct_def_instantiations[idx.0 as usize];
                        let struct_key = get_struct_key_from_def(
                            struct_inst.def,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let struct_idx = get_from_map!(&struct_key, &type_builder.struct_map);
                        let sig_idx = struct_inst.type_parameters;
                        let params = &compiled_module.signatures[sig_idx.0 as usize];
                        let type_params = params
                            .0
                            .iter()
                            .map(|type_| type_builder.make_type(module, type_))
                            .collect::<Vec<_>>();
                        Bytecode::PackGeneric(struct_idx, type_params)
                    }
                    MoveBytecode::Unpack(idx) => {
                        let struct_key = get_struct_key_from_def(
                            *idx,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let struct_idx = get_from_map!(&struct_key, &type_builder.struct_map);
                        Bytecode::Unpack(struct_idx)
                    }
                    MoveBytecode::UnpackGeneric(idx) => {
                        let struct_inst =
                            &compiled_module.struct_def_instantiations[idx.0 as usize];
                        let struct_key = get_struct_key_from_def(
                            struct_inst.def,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let struct_idx = get_from_map!(&struct_key, &type_builder.struct_map);
                        let sig_idx = struct_inst.type_parameters;
                        let params = &compiled_module.signatures[sig_idx.0 as usize];
                        let type_params = params
                            .0
                            .iter()
                            .map(|type_| type_builder.make_type(module, type_))
                            .collect::<Vec<_>>();
                        Bytecode::UnpackGeneric(struct_idx, type_params)
                    }
                    MoveBytecode::MutBorrowLoc(idx) => Bytecode::MutBorrowLoc(*idx),
                    MoveBytecode::ImmBorrowLoc(idx) => Bytecode::ImmBorrowLoc(*idx),
                    MoveBytecode::MutBorrowField(idx) => {
                        let field_handle = &compiled_module.field_handles[idx.0 as usize];
                        let struct_key = get_struct_key_from_def(
                            field_handle.owner,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let struct_idx = get_from_map!(&struct_key, &type_builder.struct_map);
                        Bytecode::MutBorrowField(FieldRef {
                            struct_idx,
                            field_idx: field_handle.field,
                        })
                    }
                    MoveBytecode::MutBorrowFieldGeneric(idx) => {
                        let field_inst = &compiled_module.field_instantiations[idx.0 as usize];
                        let field_handle =
                            &compiled_module.field_handles[field_inst.handle.0 as usize];
                        let struct_key = get_struct_key_from_def(
                            field_handle.owner,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let struct_idx = get_from_map!(&struct_key, &type_builder.struct_map);
                        let params =
                            &compiled_module.signatures[field_inst.type_parameters.0 as usize];
                        let type_params = params
                            .0
                            .iter()
                            .map(|type_| type_builder.make_type(module, type_))
                            .collect::<Vec<_>>();
                        Bytecode::MutBorrowFieldGeneric(
                            FieldRef {
                                struct_idx,
                                field_idx: field_handle.field,
                            },
                            type_params,
                        )
                    }
                    MoveBytecode::ImmBorrowField(idx) => {
                        let field_handle = &compiled_module.field_handles[idx.0 as usize];
                        let struct_key = get_struct_key_from_def(
                            field_handle.owner,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let struct_idx = get_from_map!(&struct_key, &type_builder.struct_map);
                        Bytecode::ImmBorrowField(FieldRef {
                            struct_idx,
                            field_idx: field_handle.field,
                        })
                    }
                    MoveBytecode::ImmBorrowFieldGeneric(idx) => {
                        let field_inst = &compiled_module.field_instantiations[idx.0 as usize];
                        let field_handle =
                            &compiled_module.field_handles[field_inst.handle.0 as usize];
                        let struct_key = get_struct_key_from_def(
                            field_handle.owner,
                            compiled_module,
                            &type_builder.packages[module.package],
                        );
                        let struct_idx = get_from_map!(&struct_key, &type_builder.struct_map);
                        let params =
                            &compiled_module.signatures[field_inst.type_parameters.0 as usize];
                        let type_params = params
                            .0
                            .iter()
                            .map(|type_| type_builder.make_type(module, type_))
                            .collect::<Vec<_>>();
                        Bytecode::ImmBorrowFieldGeneric(
                            FieldRef {
                                struct_idx,
                                field_idx: field_handle.field,
                            },
                            type_params,
                        )
                    }
                    MoveBytecode::ReadRef => Bytecode::ReadRef,
                    MoveBytecode::WriteRef => Bytecode::WriteRef,
                    MoveBytecode::FreezeRef => Bytecode::FreezeRef,
                    MoveBytecode::VecPack(type_, count) => {
                        let type_sig = &compiled_module.signatures[type_.0 as usize];
                        let t = type_builder.make_type(module, &type_sig.0[0]);
                        Bytecode::VecPack(t, *count)
                    }
                    MoveBytecode::VecLen(type_) => {
                        let type_sig = &compiled_module.signatures[type_.0 as usize];
                        let t = type_builder.make_type(module, &type_sig.0[0]);
                        Bytecode::VecLen(t)
                    }
                    MoveBytecode::VecImmBorrow(type_) => {
                        let type_sig = &compiled_module.signatures[type_.0 as usize];
                        let t = type_builder.make_type(module, &type_sig.0[0]);
                        Bytecode::VecImmBorrow(t)
                    }
                    MoveBytecode::VecMutBorrow(type_) => {
                        let type_sig = &compiled_module.signatures[type_.0 as usize];
                        let t = type_builder.make_type(module, &type_sig.0[0]);
                        Bytecode::VecMutBorrow(t)
                    }
                    MoveBytecode::VecPushBack(type_) => {
                        let type_sig = &compiled_module.signatures[type_.0 as usize];
                        let t = type_builder.make_type(module, &type_sig.0[0]);
                        Bytecode::VecPushBack(t)
                    }
                    MoveBytecode::VecPopBack(type_) => {
                        let type_sig = &compiled_module.signatures[type_.0 as usize];
                        let t = type_builder.make_type(module, &type_sig.0[0]);
                        Bytecode::VecPopBack(t)
                    }
                    MoveBytecode::VecUnpack(type_, count) => {
                        let type_sig = &compiled_module.signatures[type_.0 as usize];
                        let t = type_builder.make_type(module, &type_sig.0[0]);
                        Bytecode::VecUnpack(t, *count)
                    }
                    MoveBytecode::VecSwap(type_) => {
                        let type_sig = &compiled_module.signatures[type_.0 as usize];
                        let t = type_builder.make_type(module, &type_sig.0[0]);
                        Bytecode::VecSwap(t)
                    }
                    _ => panic!("Invalid bytecode found: {:?}", bytecode),
                })
                .collect();
            function.code = Some(Code { locals, code });
        }
    });
}

// Load all constants.
fn load_constants(type_builder: &TypeBuilder, modules: &mut [Module]) {
    modules.iter_mut().for_each(|module| {
        let compiled_module = module.module.as_ref().unwrap();
        module.constants = compiled_module
            .constant_pool
            .iter()
            .enumerate()
            .map(|(const_idx, constant)| {
                let type_ = type_builder.make_type(module, &constant.type_);
                Constant {
                    type_,
                    constant: ConstantPoolIndex(const_idx as u16),
                }
            })
            .collect::<Vec<_>>();
    });
}

fn build_call_graphs(
    functions: &[Function],
) -> (
    BTreeMap<FunctionIndex, BTreeSet<FunctionIndex>>,
    BTreeMap<FunctionIndex, BTreeSet<FunctionIndex>>,
) {
    let mut call_graph: BTreeMap<FunctionIndex, BTreeSet<FunctionIndex>> = BTreeMap::new();
    let mut reverse_call_graph: BTreeMap<FunctionIndex, BTreeSet<FunctionIndex>> = BTreeMap::new();
    functions.iter().for_each(|func| {
        let calls = call_graph.entry(func.self_idx).or_default();
        reverse_call_graph.entry(func.self_idx).or_default();
        if let Some(code) = func.code.as_ref() {
            code.code.iter().for_each(|bytecode| {
                if let Bytecode::Call(func_idx) | Bytecode::CallGeneric(func_idx, _) = bytecode {
                    calls.insert(*func_idx);
                    reverse_call_graph
                        .entry(*func_idx)
                        .or_default()
                        .insert(func.self_idx);
                }
            });
        }
    });
    (call_graph, reverse_call_graph)
}

// Return the key to find a `Function` in the `function_map` given a `FunctionHandleIndex`.
fn get_function_key_from_handle(
    func_handle_idx: FunctionHandleIndex,
    compiled_module: &CompiledModule,
    package: &Package,
) -> String {
    let (module_name, func_name) =
        module_function_name_from_handle(compiled_module, func_handle_idx);
    let module_id = get_package_from_function_handle(compiled_module, func_handle_idx);
    let package_id = if &module_id == compiled_module.self_id().address() {
        // call to the same package carry the origianl package id.
        // Change that to the current package id which is the correct behavior and
        // the function may not even exist in the original package.
        package.id
    } else {
        // Relocate the call to the right package
        let package_id = ObjectID::from(module_id);
        match package
            .package
            .as_ref()
            .unwrap()
            .linkage_table()
            .get(&package_id)
        {
            None => package_id,
            Some(upgrade_info) => upgrade_info.upgraded_id,
        }
    };
    format!("{}::{}::{}", package_id, module_name, func_name)
}

// Return the key to find a `Struct` in the `struct_map` given a `StructDefinitionIndex`.
fn get_struct_key_from_def(
    struct_def_idx: StructDefinitionIndex,
    compiled_module: &CompiledModule,
    package: &Package,
) -> String {
    let (module_name, struct_name) = module_struct_name_from_def(compiled_module, struct_def_idx);
    let key = (module_name.to_string(), struct_name.to_string());
    let package_id = get_package_from_struct_def(compiled_module, struct_def_idx);
    // properly forward to the right package for type that have been introduced
    // in a later version of the package
    let package_id = match package.type_origin.get(&key) {
        None => ObjectID::from(package_id),
        Some(package_id) => *package_id,
    };
    format!("{}::{}::{}", package_id, module_name, struct_name)
}

//
// Utilities to check consistency of package versions
//

// Assert that all modules in a package have the same address.
fn check_package(package: &Package, modules: &[Module]) {
    let package_id = package.id;
    let mut ids = BTreeSet::new();
    package.modules.iter().for_each(|module_idx| {
        let module = &modules[*module_idx];
        ids.insert(*module.module_id.address());
    });
    assert_eq!(
        ids.len(),
        1,
        "modules in package {} have different/multiple origins: {:?}",
        package_id,
        ids,
    );
}

// Verify that all versions are correct.
// Packages at version 1 must have a `root_version` that is `None`.
// All versioned packages (loaded in `versions` of the root) must have a `root_version` that
// is the package at version 1 and `versions` must be an empty vector for non root.
fn verify_versions(packages: &[Package]) {
    packages.iter().for_each(|package| {
        let package_id = &package.id;
        // framework package are unique in the system
        if FRAMEWORK.contains(package_id) {
            assert!(
                package.versions.is_empty(),
                "framework must have one version only"
            );
            assert!(
                package.root_version.is_none(),
                "framework must have one version only"
            );
            return;
        }
        if package.version == 1 {
            // a root package, may or may not have subsequent versions
            assert!(
                package.root_version.is_none(),
                "package {} at version 1 must not have a root (it is the root version)",
                package_id,
            );
            // all versions, if any, must point back to root
            package.versions.iter().for_each(|version| {
                let versioned = &packages[*version];
                assert_eq!(
                    versioned.root_version,
                    Some(package.self_idx),
                    "package {} at version {} must point to its root {}",
                    versioned.id,
                    versioned.version,
                    package_id,
                )
            })
        } else {
            // versioned package
            assert!(
                package.versions.is_empty(),
                "non root package {} must have no package in its field `versions`",
                package_id
            );
            assert!(
                package.root_version.is_some(),
                "non root package {} must point to a root",
                package_id
            );
            // grab the root and see that the package is in there and at the right position
            let root_package = &packages[package.root_version.unwrap()];
            assert!(
                root_package.versions.contains(&package.self_idx),
                "root package {} must contain versioned package {}",
                root_package.id,
                package_id,
            );
            let version = package.version as usize;
            assert_eq!(
                package.self_idx,
                root_package.versions[version - 2],
                "package {} at version {} must be at index {} in root package {}",
                package_id,
                version,
                version - 2,
                root_package.id,
            )
        }
    });
}
