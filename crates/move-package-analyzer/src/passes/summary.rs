// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Save summary/useful info about all the packages in the specific network.
/// There is no real format on this pass, it just prints info of interest.
/// Feel free to change as you need.
use crate::{
    model::{
        global_env::GlobalEnv, model_utils::bytecode_to_string, move_model::Bytecode,
        walkers::walk_bytecodes,
    },
    write_to, FRAMEWORK,
};
use move_binary_format::file_format::{AbilitySet, Visibility};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};
use sui_types::base_types::ObjectID;
use tracing::error;

pub(crate) fn run(env: &GlobalEnv, output: &Path) {
    let file = File::create(output.join("summary.txt"))
        .unwrap_or_else(|_| panic!("Unable to create file summary.txt in {}", output.display()));
    let buffer = &mut BufWriter::new(file);

    write_to!(buffer, "Environment Summary");
    package_summary(env, buffer);
    type_summary(env, buffer);
    function_summary(env, buffer);

    buffer.flush().unwrap();
}

//
// Package summary
//
fn package_summary(env: &GlobalEnv, buffer: &mut BufWriter<File>) {
    let mut module_count = 0;
    let mut struct_count = 0;
    let mut func_count = 0;
    let mut const_count = 0;
    let mut versioned = 0;
    let mut upgrades = 0;
    let mut unknown = 0;
    let mut direct_dependencies: BTreeMap<ObjectID, usize> = BTreeMap::new();
    let mut indirect_dependencies: BTreeMap<ObjectID, usize> = BTreeMap::new();
    env.packages.iter().for_each(|package| {
        if !FRAMEWORK.contains(&package.id) {
            const FIRST_VERSION: u64 = 1;
            match package.version.cmp(&FIRST_VERSION) {
                std::cmp::Ordering::Greater => {
                    upgrades += 1;
                }
                std::cmp::Ordering::Equal => {
                    if !package.versions.is_empty() {
                        versioned += 1;
                    }
                }
                std::cmp::Ordering::Less => {
                    // there should be (and are not) any unknown package when importing the chain
                    unknown += 1;
                }
            }
        }
        module_count += package.modules.len();
        env.modules_in_package(package).for_each(|module| {
            struct_count += module.structs.len();
            func_count += module.functions.len();
            const_count += module.constants.len();
        });
        package.direct_dependencies.iter().for_each(|dep| {
            *direct_dependencies
                .entry(env.packages[*dep].id)
                .or_insert(0) += 1;
        });
        package
            .dependencies
            .difference(&package.direct_dependencies)
            .for_each(|dep| {
                *indirect_dependencies
                    .entry(env.packages[*dep].id)
                    .or_insert(0) += 1;
            });
    });
    write_to!(
        buffer,
        "\n\
        * packages: {}, versioned: {}, upgrades: {}, unknown: {}\n\
        ** modules: {}, structs: {}, functions: {}, constants: {}",
        env.packages.len(),
        versioned,
        upgrades,
        unknown,
        module_count,
        struct_count,
        func_count,
        const_count,
    );
    write_to!(
        buffer,
        "** direct dependencies: {}, indirect dependencies {}",
        direct_dependencies.len(),
        indirect_dependencies.len(),
    );
    write_to!(buffer, "** direct dependencies:");
    let mut sorted_direct = direct_dependencies.iter().collect::<Vec<_>>();
    sorted_direct.sort_by(|(_, count1), (_, count2)| count1.cmp(count2).reverse());
    sorted_direct[0..20].iter().for_each(|(id, count)| {
        write_to!(
            buffer,
            "[{}] {} [{}]",
            count,
            id,
            env.packages[env.package_map[*id]].version,
        );
    });
    write_to!(buffer, "** indirect dependencies:");
    let mut sorted_indirect = indirect_dependencies.iter().collect::<Vec<_>>();
    sorted_indirect.sort_by(|(_, count1), (_, count2)| count1.cmp(count2).reverse());
    sorted_indirect[0..20].iter().for_each(|(id, count)| {
        write_to!(
            buffer,
            "[{}] {} [{}]",
            count,
            id,
            env.packages[env.package_map[*id]].version,
        );
    });
}

//
// Type summary
//
fn type_summary(env: &GlobalEnv, buffer: &mut BufWriter<File>) {
    let struct_count = env.structs.len();
    let mut key_count = 0;
    let mut key_no_store_count = 0;
    let mut store_count = 0;
    env.structs.iter().for_each(|struct_| {
        if AbilitySet::has_key(struct_.abilities) {
            key_count += 1;
            if !AbilitySet::has_store(struct_.abilities) {
                key_no_store_count += 1;
            }
        }
        if AbilitySet::has_store(struct_.abilities) {
            store_count += 1;
        }
    });
    write_to!(
        buffer,
        "\n\
        * structs: {}, key: {}, key_no_store: {}, store: {}",
        struct_count,
        key_count,
        key_no_store_count,
        store_count,
    );
}

//
// Function summary
//
fn function_summary(env: &GlobalEnv, buffer: &mut BufWriter<File>) {
    let mut native = 0usize;
    let mut public = 0usize;
    let mut friend = 0usize;
    let mut private = 0usize;
    let mut private_entry = 0usize;
    let mut public_entry = 0usize;
    let mut friend_entry = 0usize;
    env.functions.iter().for_each(|function| {
        match function.visibility {
            Visibility::Public => {
                public += 1;
                if function.is_entry {
                    public_entry += 1;
                }
            }
            Visibility::Friend => {
                friend += 1;
                if function.is_entry {
                    friend_entry += 1;
                }
            }
            Visibility::Private => {
                private += 1;
                if function.is_entry {
                    private_entry += 1;
                }
            }
        }
        if function.code.is_none() {
            native += 1;
        }
    });
    let entry = public_entry + friend_entry + private_entry;
    write_to!(
        buffer,
        "\n\
        * functions: {}, public: {}, package: {}, private: {}, native: {}\n\
        ** entries: {}, public: {}, package: {}, private: {}",
        env.functions.len(),
        public,
        friend,
        private,
        native,
        entry,
        public_entry,
        friend_entry,
        private_entry,
    );

    count_calls(env, buffer);
    count_bytecodes(env, buffer);
}

fn count_calls(env: &GlobalEnv, buffer: &mut BufWriter<File>) {
    let mut total_calls = 0usize;
    let mut in_module_calls = 0usize;
    let mut in_package_calls = 0usize;
    let mut framwork_calls = 0usize;
    let mut external_calls = 0usize;
    walk_bytecodes(env, |env, func, bytecode| match bytecode {
        Bytecode::Call(func_idx) | Bytecode::CallGeneric(func_idx, _) => {
            total_calls += 1;
            let callee = &env.functions[*func_idx];
            if callee.module == func.module {
                in_module_calls += 1;
            } else if callee.package == func.package {
                in_package_calls += 1;
            } else if env.framework.contains_key(&callee.package) {
                framwork_calls += 1;
            } else {
                external_calls += 1;
            }
        }
        _ => (),
    });
    write_to!(
        buffer,
        "** calls: {}, in module: {}, in package: {}, framework: {}, cross package: {}",
        total_calls,
        in_module_calls,
        in_package_calls,
        framwork_calls,
        external_calls,
    );
}

/// Count bytecodes
fn count_bytecodes(env: &GlobalEnv, buffer: &mut BufWriter<File>) {
    let mut bytecodes = BTreeMap::new();
    let mut count = 0usize;
    walk_bytecodes(env, |_, _, bytecode| {
        count += 1;
        insert_bytecode(&mut bytecodes, bytecode);
    });
    write_to!(buffer, "** bytecodes: {count}");
    let mut entries: Vec<_> = bytecodes.into_iter().collect();
    entries.sort_by_key(|&(_, count)| count);
    write_to!(buffer, "\t{:<25}{:>8}", "Bytecode", "Count");
    for (name, count) in entries.iter().rev() {
        write_to!(buffer, "\t{:<25}{:>8}", name, count);
    }
}

fn insert_bytecode(bytecodes: &mut BTreeMap<String, usize>, bytecode: &Bytecode) {
    let name = bytecode_to_string(bytecode);
    let count = bytecodes.entry(name).or_insert(0);
    *count += 1;
}
