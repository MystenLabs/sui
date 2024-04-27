// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Study versions across packages and build a version summary.
/// IN PROGRESS.....
use crate::{
    model::{
        global_env::GlobalEnv,
        model_utils::type_name,
        move_model::{FunctionIndex, Package, PackageIndex, StructIndex},
    },
    write_to,
};
use move_binary_format::file_format::{AbilitySet, Visibility};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as FmtWrite,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};
use sui_types::base_types::ObjectID;
use tracing::error;

pub(crate) fn run(env: &GlobalEnv, output: &Path) {
    let file = File::create(output.join("versions.txt"))
        .unwrap_or_else(|_| panic!("Unable to create file versions.txt in {}", output.display()));
    let buffer = &mut BufWriter::new(file);

    write_to!(buffer, "Package Versions",);

    //
    // Write count of package with a later version and how many upgrades
    let mut upgrades = 0;
    let versions: Vec<_> = env
        .packages
        .iter()
        .filter(|package| !package.versions.is_empty())
        .map(|package| {
            assert_eq!(package.version, 1, "Non root package {}", package.id);
            upgrades += package.versions.len();
            (package.self_idx, package.versions.clone())
        })
        .collect();
    write_to!(
        buffer,
        "* versioned packages: {}, upgrades: {}",
        versions.len(),
        upgrades,
    );

    //
    // Write the map from upgrade count to root packages
    let mut upgrades_count_map: BTreeMap<usize, Vec<PackageIndex>> = BTreeMap::new();
    versions.iter().for_each(|(package_idx, versions)| {
        let upgrades = upgrades_count_map.entry(versions.len()).or_default();
        upgrades.push(*package_idx);
    });
    let mut upgrade_counter = upgrades_count_map.into_iter().collect::<Vec<_>>();
    upgrade_counter.sort_by(|e1, e2| {
        if e1.1.len() != e2.1.len() {
            e1.1.len().cmp(&e2.1.len())
        } else {
            e1.0.cmp(&e2.0)
        }
    });
    write_to!(buffer, "==================== UPGRADES ===================");
    upgrade_counter.iter().rev().for_each(|(count, roots)| {
        let root_pacakges: Vec<_> = roots
            .iter()
            .map(|root| env.packages[*root].id.to_string())
            .collect();
        write_to!(buffer, "{} upgrades[{}]", count, root_pacakges.len(),);
    });
    study_protocols(env, buffer);

    buffer.flush().unwrap();
}

//
// Protocols non-sense
// TODO: re-enable and make sense of the non-sense
//

fn study_protocols(env: &GlobalEnv, buffer: &mut BufWriter<File>) {
    let protocols = read_protocols(env);
    // write_to!(
    //     buffer,
    //     "================ VERSION UPGRADES ===================="
    // );
    // protocols.iter().for_each(|protocol| {
    //     write_to!(buffer, "== Package: {}", protocol.id);
    //     protocol
    //         .protocols
    //         .iter()
    //         .enumerate()
    //         .for_each(|(ver, version_protocol)| {
    //             write_to!(buffer, "V{}: {}", ver + 1, version_protocol.id);
    //             version_protocol.modules.iter().for_each(|module_protocol| {
    //                 write_to!(
    //                     buffer,
    //                     "  Module: {}, Public: {}, Entry: {}, Types: {}, Key Types: {}",
    //                     module_protocol.name,
    //                     module_protocol.public.len(),
    //                     module_protocol.entry.len(),
    //                     module_protocol.types.len(),
    //                     module_protocol.key_types.len(),
    //                 );
    //             });
    //         });
    // });
    // write_to!(
    //     buffer,
    //     "================ PROTOCOL UPGRADES ===================="
    // );
    // protocols.iter().for_each(|protocol| {
    //     write_to!(
    //         buffer,
    //         "== Package: {} [{}]",
    //         protocol.id,
    //         protocol.protocols.len()
    //     );
    //     let mut versioned_modules: BTreeMap<String, Vec<(usize, ModuleProtocol)>> = BTreeMap::new();
    //     protocol
    //         .protocols
    //         .iter()
    //         .enumerate()
    //         .for_each(|(ver, version_protocol)| {
    //             version_protocol.modules.iter().for_each(|module_protocol| {
    //                 let modules = versioned_modules
    //                     .entry(module_protocol.name.clone())
    //                     .or_default();
    //                 modules.push((ver + 1, module_protocol.clone()));
    //             });
    //         });
    //     versioned_modules.iter().for_each(|(name, modules)| {
    //         write_to!(buffer, "Module: {}", name);
    //         let mut current_public = 0;
    //         let mut current_entry = 0;
    //         let mut current_types = 0;
    //         let mut current_key_types = 0;
    //         modules.iter().for_each(|(ver, module_protocol)| {
    //             let mut print = false;
    //             let mut protocol = format!("  V{}:", ver);
    //             if module_protocol.public.len() > current_public {
    //                 protocol = format!("{} Public: {},", protocol, module_protocol.public.len());
    //                 current_public = module_protocol.public.len();
    //                 print = true;
    //             }
    //             if module_protocol.entry.len() > current_entry {
    //                 protocol = format!("{} Entry: {},", protocol, module_protocol.entry.len());
    //                 current_entry = module_protocol.entry.len();
    //                 print = true;
    //             }
    //             if module_protocol.types.len() > current_types {
    //                 protocol = format!("{} Types: {},", protocol, module_protocol.types.len());
    //                 current_types = module_protocol.types.len();
    //                 print = true;
    //             }
    //             if module_protocol.key_types.len() > current_key_types {
    //                 protocol = format!(
    //                     "{} Key Types: {},",
    //                     protocol,
    //                     module_protocol.key_types.len()
    //                 );
    //                 current_key_types = module_protocol.key_types.len();
    //                 print = true;
    //             }
    //             if print {
    //                 write_to!(buffer, "{}", protocol);
    //             }
    //         });
    //     });
    // });
    write_to!(buffer, "================ VERSIONS ====================");
    protocols.iter().for_each(|protocol| {
        write_to!(
            buffer,
            "== Package: {} [{}]",
            protocol.id,
            protocol.protocols.len()
        );
        let mut versioned_modules: BTreeMap<String, Vec<(usize, ModuleProtocol)>> = BTreeMap::new();
        protocol
            .protocols
            .iter()
            .enumerate()
            .for_each(|(ver, version_protocol)| {
                version_protocol.modules.iter().for_each(|module_protocol| {
                    let modules = versioned_modules
                        .entry(module_protocol.name.clone())
                        .or_default();
                    modules.push((ver + 1, module_protocol.clone()));
                });
            });
        versioned_modules.iter().for_each(|(name, modules)| {
            write_to!(buffer, "{}:", name);
            let mut public = BTreeSet::new();
            let mut entry = BTreeSet::new();
            let mut types = BTreeSet::new();
            modules.iter().for_each(|(ver, module_protocol)| {
                let indent = "    ".to_string();
                let mut write = false;
                let mut protocol = format!("  V{}:", ver);
                module_protocol.key_types.iter().for_each(|struct_idx| {
                    let type_name = env.structs[*struct_idx].name;
                    if !types.contains(&type_name) {
                        protocol = format!(
                            "{}\n{}{}",
                            protocol,
                            indent,
                            type_proto(env, *struct_idx, &indent),
                        );
                        types.insert(type_name);
                        write = true;
                    }
                });
                module_protocol.types.iter().for_each(|struct_idx| {
                    let type_name = env.structs[*struct_idx].name;
                    if !types.contains(&type_name) {
                        protocol = format!(
                            "{}\n{}{}",
                            protocol,
                            indent,
                            type_proto(env, *struct_idx, &indent),
                        );
                        types.insert(type_name);
                        write = true;
                    }
                });
                module_protocol.public.iter().for_each(|func_idx| {
                    let func_name = env.functions[*func_idx].name;
                    if !public.contains(&func_name) {
                        protocol =
                            format!("{}\n{}{}", protocol, indent, function_proto(env, *func_idx),);
                        public.insert(func_name);
                        write = true;
                    }
                });
                module_protocol.entry.iter().for_each(|func_idx| {
                    let func_name = env.functions[*func_idx].name;
                    if !entry.contains(&func_name) {
                        protocol =
                            format!("{}\n{}{}", protocol, indent, function_proto(env, *func_idx),);
                        entry.insert(func_name);
                        write = true;
                    }
                });
                if write {
                    write_to!(buffer, "{}", protocol);
                }
            });
        });
    });
}

fn type_proto(env: &GlobalEnv, struct_idx: StructIndex, indent: &str) -> String {
    let struct_ = &env.structs[struct_idx];
    let abilities = if struct_.abilities != AbilitySet::EMPTY {
        format!("has {}", pretty_abilities(struct_.abilities))
    } else {
        "".to_string()
    };
    let struct_name = if struct_.type_parameters.is_empty() {
        format!("struct {} {}", env.struct_name(struct_), abilities)
    } else {
        format!(
            "struct {}<{}> {}",
            env.struct_name(struct_),
            struct_
                .type_parameters
                .iter()
                .enumerate()
                .map(|(idx, abilities)| {
                    let phantom = if abilities.is_phantom { "phantom " } else { "" };
                    if abilities.constraints == AbilitySet::EMPTY {
                        format!("{} {}", phantom, idx)
                    } else {
                        format!(
                            "{}{}: {}",
                            phantom,
                            idx,
                            pretty_abilities(abilities.constraints)
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            abilities,
        )
    };
    let struct_with_fields: String =
        struct_
            .fields
            .iter()
            .fold(String::new(), |mut output, field| {
                let _ = write!(
                    output,
                    "\n{indent}  {}: {}",
                    env.field_name(field),
                    type_name(env, &field.type_),
                );
                output
            });
    format!("{}{}", struct_name, struct_with_fields,)
}

#[derive(Clone, Debug)]
struct PackageProtocol {
    id: ObjectID, // the root id
    // all versions of this package, including the root
    protocols: Vec<VersionProtocol>,
}

#[derive(Clone, Debug)]
struct VersionProtocol {
    #[allow(dead_code)]
    id: ObjectID,
    modules: Vec<ModuleProtocol>,
}

#[derive(Clone, Debug)]
struct ModuleProtocol {
    name: String,                // module name must match future version
    public: Vec<FunctionIndex>,  // all public function in a module, entry or not
    entry: Vec<FunctionIndex>,   // all non public entries
    types: Vec<StructIndex>,     // all types in a module
    key_types: Vec<StructIndex>, // all key types in a module
}

impl ModuleProtocol {
    fn new(name: String) -> Self {
        Self {
            name,
            public: vec![],
            entry: vec![],
            types: vec![],
            key_types: vec![],
        }
    }
}

fn read_protocols(env: &GlobalEnv) -> Vec<PackageProtocol> {
    env.packages
        .iter()
        .filter(|package| !package.versions.is_empty())
        .map(|package| {
            assert_eq!(package.version, 1, "Non root package {}", package.id);
            let mut protocol = PackageProtocol {
                id: package.id, // root package id
                protocols: vec![],
            };
            protocol.protocols.push(read_package_protocol(env, package));
            package.versions.iter().for_each(|version| {
                let upgrade = &env.packages[*version];
                protocol.protocols.push(read_package_protocol(env, upgrade));
            });
            protocol
        })
        .collect()
}

fn read_package_protocol(env: &GlobalEnv, package: &Package) -> VersionProtocol {
    let modules = env
        .modules_in_package(package)
        .map(|module| {
            let module_name = env.module_name(module);
            let mut module_protocol = ModuleProtocol::new(module_name.clone());
            module.functions.iter().for_each(|func_idx| {
                let func = &env.functions[*func_idx];
                if func.visibility == Visibility::Public {
                    module_protocol.public.push(*func_idx);
                } else if func.is_entry {
                    module_protocol.entry.push(*func_idx);
                }
            });
            module.structs.iter().for_each(|struct_idx| {
                let struct_ = &env.structs[*struct_idx];
                if struct_.abilities.has_key() {
                    module_protocol.key_types.push(*struct_idx);
                }
                module_protocol.types.push(*struct_idx);
            });
            module_protocol
        })
        .collect();
    VersionProtocol {
        id: package.id,
        modules,
    }
}

fn function_proto(env: &GlobalEnv, func_idx: FunctionIndex) -> String {
    let func = &env.functions[func_idx];
    let func_entry = if func.is_entry {
        "entry fun".to_string()
    } else {
        "fun".to_string()
    };
    let func_vis = match func.visibility {
        Visibility::Friend => format!("package {}", func_entry),
        Visibility::Public => format!("public {}", func_entry),
        Visibility::Private => func_entry,
    };
    let params = if func.parameters.is_empty() {
        "".to_string()
    } else {
        func.parameters
            .iter()
            .map(|type_| type_name(env, type_))
            .collect::<Vec<_>>()
            .join(", ")
            .to_string()
    };
    let func_proto = if func.type_parameters.is_empty() {
        format!("{} {}({})", func_vis, env.function_name(func), params)
    } else {
        format!(
            "{} {}<{}>({})",
            func_vis,
            env.function_name(func),
            func.type_parameters
                .iter()
                .enumerate()
                .map(|(idx, ability_set)| {
                    if ability_set == &AbilitySet::EMPTY {
                        format!("{}", idx)
                    } else {
                        format!("{}: {}", idx, pretty_abilities(*ability_set))
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            params,
        )
    };
    if func.returns.is_empty() {
        func_proto
    } else {
        format!(
            "{}: {}",
            func_proto,
            func.returns
                .iter()
                .map(|type_| type_name(env, type_))
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

fn pretty_abilities(ability_set: AbilitySet) -> String {
    let mut abilities = vec![];
    if ability_set == AbilitySet::EMPTY {
        return "".to_string();
    }
    if ability_set.has_key() {
        abilities.push("key");
    }
    if ability_set.has_store() {
        abilities.push("store");
    }
    if ability_set.has_copy() {
        abilities.push("copy");
    }
    if ability_set.has_drop() {
        abilities.push("drop");
    }
    abilities.join(", ")
}
