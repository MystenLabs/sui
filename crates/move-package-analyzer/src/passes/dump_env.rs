// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Generate a `environment.txt` file in the output directory.
/// The file is a summary of all packages in the environment.
/// Packages, modules, structs, functions and dependencies are available in
/// a reasonable and easy to read format.
use crate::{
    model::{
        global_env::GlobalEnv,
        model_utils::type_name,
        move_model::{FunctionIndex, Module},
    },
    write_to,
};
use move_binary_format::file_format::{AbilitySet, Visibility};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};
use tracing::error;

/// Write `GlobalEnv` to `environment.txt` file.
pub fn run(env: &GlobalEnv, output: &Path) {
    environment(env, output);
}

fn environment(env: &GlobalEnv, output: &Path) {
    let file = File::create(output.join("environment.txt")).unwrap_or_else(|_| {
        panic!(
            "Unable to create file environment.txt in {}",
            output.display()
        )
    });
    let buffer = &mut BufWriter::new(file);

    // loop through all packages
    for package in env.packages.iter() {
        let move_package = package.package.as_ref().unwrap();
        // print package id and version
        write_to!(buffer, "package {} [{}]", package.id, package.version);
        if let Some(root) = package.root_version {
            // print root package id if any
            write_to!(buffer, "\tpackage first version: {}", env.packages[root].id);
        };
        // print type origin table
        write_to!(buffer, "\ttype origins:");
        package
            .type_origin
            .iter()
            .for_each(|((module, type_), origin)| {
                write_to!(
                    buffer,
                    "\t\t{}::{} -> {} [{}]",
                    module,
                    type_,
                    origin,
                    env.packages[env.package_map[origin]].version,
                );
            });
        // linkage table
        write_to!(buffer, "\tdependencies:");
        move_package
            .linkage_table()
            .iter()
            .for_each(|(module, version)| {
                write_to!(
                    buffer,
                    "\t\t{} -> {} [{}]",
                    module,
                    version.upgraded_id,
                    version.upgraded_version.value(),
                );
            });
        write_to!(buffer, "\tdirect dependencies:");
        package.direct_dependencies.iter().for_each(|version| {
            write_to!(
                buffer,
                "\t\t{} [{}]",
                env.packages[*version].id,
                env.packages[*version].version,
            );
        });
        write_to!(buffer, "\tindirect dependencies:");
        package
            .dependencies
            .difference(&package.direct_dependencies)
            .for_each(|dep| {
                write_to!(
                    buffer,
                    "\t\t{} [{}]",
                    env.packages[*dep].id,
                    env.packages[*dep].version,
                );
            });
        // print modules
        for module_idx in &package.modules {
            let module = &env.modules[*module_idx];
            write_to!(buffer, "\tmodule {}", env.module_name(module));
            print_structs(env, module, buffer);
            print_functions(env, module, buffer);
        }
    }

    buffer.flush().unwrap();
}

fn print_structs(env: &GlobalEnv, module: &Module, buffer: &mut BufWriter<File>) {
    for struct_idx in &module.structs {
        let struct_ = &env.structs[*struct_idx];
        let abilities = if struct_.abilities != AbilitySet::EMPTY {
            format!("has {}", pretty_abilities(struct_.abilities))
        } else {
            "".to_string()
        };
        if struct_.type_parameters.is_empty() {
            write_to!(
                buffer,
                "\t\tstruct {} {}",
                env.struct_name(struct_),
                abilities
            );
        } else {
            write_to!(
                buffer,
                "\t\tstruct {}<{}> {}",
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
            );
        }
        for field in &struct_.fields {
            write_to!(
                buffer,
                "\t\t\t{}: {}",
                env.field_name(field),
                type_name(env, &field.type_),
            );
        }
    }
}

fn print_functions(env: &GlobalEnv, module: &Module, buffer: &mut BufWriter<File>) {
    let (public_entries, public, friend_entry, friend, private_entry, private) =
        module.functions.iter().fold(
            (vec![], vec![], vec![], vec![], vec![], vec![]),
            |(
                mut public_entries,
                mut public,
                mut friend_entry,
                mut friend,
                mut private_entry,
                mut private,
            ),
             func_idx| {
                let func = &env.functions[*func_idx];
                match func.visibility {
                    Visibility::Friend => {
                        if func.is_entry {
                            friend_entry.push(*func_idx);
                        } else {
                            friend.push(*func_idx);
                        }
                    }
                    Visibility::Public => {
                        if func.is_entry {
                            public_entries.push(*func_idx);
                        } else {
                            public.push(*func_idx);
                        }
                    }
                    Visibility::Private => {
                        if func.is_entry {
                            private_entry.push(*func_idx);
                        } else {
                            private.push(*func_idx);
                        }
                    }
                }
                (
                    public_entries,
                    public,
                    friend_entry,
                    friend,
                    private_entry,
                    private,
                )
            },
        );
    public
        .iter()
        .for_each(|func_idx| write_function(env, *func_idx, buffer));
    public_entries
        .iter()
        .for_each(|func_idx| write_function(env, *func_idx, buffer));
    friend_entry
        .iter()
        .for_each(|func_idx| write_function(env, *func_idx, buffer));
    private_entry
        .iter()
        .for_each(|func_idx| write_function(env, *func_idx, buffer));
    friend
        .iter()
        .for_each(|func_idx| write_function(env, *func_idx, buffer));
    private
        .iter()
        .for_each(|func_idx| write_function(env, *func_idx, buffer));

    fn write_function(env: &GlobalEnv, func_idx: FunctionIndex, buffer: &mut BufWriter<File>) {
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
            format!("\t\t{} {}({})", func_vis, env.function_name(func), params)
        } else {
            format!(
                "\t\t{} {}<{}>({})",
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
            write_to!(buffer, "{}", func_proto);
        } else {
            write_to!(
                buffer,
                "{}: {}",
                func_proto,
                func.returns
                    .iter()
                    .map(|type_| type_name(env, type_))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }
}

// Show abilities in a readable format.
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
