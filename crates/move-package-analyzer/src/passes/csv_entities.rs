// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Generate CSV files with information about Move "entities/type system" constructs.
/// At the moment packages, modules, compiled modules, functions and structs are supported.
/// The CSV files are generated in the output directory specified in the `passes.yaml` file.
/// The CSV files can be imported in a spreadsheet for further analysis.
/// Each file is generated in its own function that performs a simple walk of the
/// "entity" in question (packages, modules, etc.).
/// More information can be added as needed by changing the given function.
/// As a general rule, we want to avoid renaming columns in the CSV so code
/// in `scripts` keeps working (python scripts looking at csv files).
use crate::{
    model::{global_env::GlobalEnv, move_model::Bytecode},
    write_to,
};
use move_binary_format::file_format::Visibility;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};
use tracing::error;

pub(crate) fn run(env: &GlobalEnv, output: &Path) {
    packages(env, output);
    modules(env, output);
    binary_modules(env, output);
    functions(env, output);
    structs(env, output);
}

/// Generate a `packages.csv` file with package information.
fn packages(env: &GlobalEnv, output: &Path) {
    let file = File::create(output.join("packages.csv"))
        .unwrap_or_else(|_| panic!("Unable to create file packages.csv in {}", output.display()));
    let buffer = &mut BufWriter::new(file);

    // columns:
    //   package_id                         the package id (object id) can be used to retrieve the package from the DB
    //   version                            the version number of the package (starting from 1)
    //   root_version                       the package is (object id) of the package at version 1,
    //                                      or `package_id` if the package is version 1
    //   dependencies                       number of dependencies for this package as defined in the linkage table
    //   versioned_dependencies             number of dependencies for this package that are versioned
    //                                      (i.e. not the root version, version > 1)
    //   direct_dependencies                number of direct dependencies for this package.
    //                                      Those are dependencies used by code in the package
    //   indirect_dependencies              number of indirect dependencies for this package. Those are all
    //                                      dependencies used by dependent packages and not the package itself
    //   type_dependencies                  dependence related to type origin. Types are repeated/duplicated
    //                                      in every package, so this dependencies are not used and a bit a
    //                                      side effect of our current implementation
    //   origin_tables                      for every type defined in the package maps the type to the package
    //                                      where the type was defined
    //   modules                            Number of modules in the package
    //   structs                            Number of structs in the package
    //   functions                          Number of functions in the package
    //   constants                          Number of constants in the package
    //   public_functions                   Number of public functions in the package
    //   entries                            Number of public functions that are also entry points
    //   non_public_entries                 Number of non public functions that are entry points
    //   in_package_calls                   Number of calls to functions in the same package
    //   cross_package_calls                Number of calls to functions in other packages, except framework packages
    //   framework_calls                    Number of calls to functions in the framework
    write_to!(
        buffer,
        "package_id,version,root_version,\
        dependencies,versioned_dependencies,direct_dependencies,indirect_dependencies,\
        origin_tables,\
        modules,structs,functions,constants,\
        public_functions,entries,non_public_entries,\
        in_package_calls,cross_package_calls,framework_calls"
    );

    // loop over packages
    env.packages.iter().for_each(|package| {
        // loop over modules in package
        let mut struct_count = 0usize;
        let mut func_count = 0usize;
        let mut const_count = 0usize;
        env.modules_in_package(package).for_each(|module| {
            struct_count += module.structs.len();
            func_count += module.functions.len();
            const_count += module.constants.len();
        });

        // loop over functions in package
        let mut public = 0usize;
        let mut entries = 0usize;
        let mut non_public_entries = 0usize;
        let mut in_package_calls = 0usize;
        let mut cross_package_calls = 0usize;
        let mut framework_calls = 0usize;
        env.functions_in_package(package).for_each(|func| {
            // public/entry info
            if func.visibility == Visibility::Public {
                public += 1;
                if func.is_entry {
                    entries += 1;
                }
            } else if func.is_entry {
                non_public_entries += 1;
            }
            // call distribution
            if let Some(code) = func.code.as_ref() {
                code.code.iter().for_each(|bytecode| match bytecode {
                    Bytecode::Call(func_idx) | Bytecode::CallGeneric(func_idx, _) => {
                        let func = &env.functions[*func_idx];
                        let func_pkg_idx = func.package;
                        if env.framework.contains_key(&func_pkg_idx) {
                            framework_calls += 1;
                        } else if package.direct_dependencies.contains(&func_pkg_idx) {
                            cross_package_calls += 1;
                        } else {
                            assert!(
                                !package.dependencies.contains(&func_pkg_idx),
                                "Unexpected dependency {} in package {}",
                                env.packages[func_pkg_idx].id,
                                package.id,
                            );
                            in_package_calls += 1;
                        }
                    }
                    _ => (), // continue
                });
            };
        });

        // dependencies, versioned_dependencies, direct_dependencies, indirect_dependencies, type_dependencies
        let dependencies_count = package.dependencies.len();
        let versioned_dependencies = package
            .linkage_table
            .iter()
            .map(|(origin, dest)| origin != dest)
            .filter(|&is_versioned| is_versioned)
            .count();
        let direct_dependencies = package.direct_dependencies.len();
        let indirect_dependencies = package
            .dependencies
            .difference(&package.direct_dependencies)
            .count();

        package
            .dependencies
            .difference(&package.direct_dependencies)
            .count();
        write_to!(
            buffer,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            // package_id, version, root_version,
            package.id,
            package.version,
            env.packages[package.root_version.unwrap_or(package.self_idx)].id,
            // dependencies, versioned_dependencies, direct_dependencies, indirect_dependencies, type_dependencies,
            dependencies_count,
            versioned_dependencies,
            direct_dependencies,
            indirect_dependencies,
            // origin_tables,
            package.type_origin.len(),
            // modules, structs, functions, constants
            package.modules.len(),
            struct_count,
            func_count,
            const_count,
            // public_functions, entries, non_public_entries
            public,
            entries,
            non_public_entries,
            // internal_calls, cross_package_calls
            in_package_calls,
            cross_package_calls,
            framework_calls,
        );
    });

    buffer.flush().unwrap();
}

/// Generate a `modules.csv` file with module information.
fn modules(env: &GlobalEnv, output: &Path) {
    let file = &mut File::create(output.join("modules.csv"))
        .unwrap_or_else(|_| panic!("Unable to create file modules.csv in {}", output.display()));
    let buffer = &mut BufWriter::new(file);

    // columns:
    //   package                            package id the module belongs to
    //   version                            version of the package (starting from 1)
    //   module                             ModuleId as in the binary format
    //   dependencies                       count of dependencies for this module (excluding self)
    //   type_dependencies                  count of type dependencies for this module,
    //                                      which packages are types first defined
    //   structs                            count of structs in module
    //   functions                          count of functions in module
    //   constants,                         count of constants in module
    //   key_structs                        count of struct with `key` ability
    //   store_structs                      count of struct with `store` ability (and no `key`
    //   other_structs                      count of struct with no `key` and no `store`
    //   public_functions                   count of public functions in module
    //   entries                            count of public functions that are also entry points
    //   non_public_entries                 count of non public functions that are entry points
    //   in_module_calls                    count of calls to functions in the same module
    //   in_package_calls                   count of calls to functions in the same package
    //   cross_package_calls                count of calls to functions in other packages, except framework packages
    //   framework_calls                    count of calls to functions in the framework
    write_to!(
        buffer,
        "package,version,module,\
        dependencies,\
        structs,functions,constants,\
        key_structs,store_structs,other_structs,\
        public_functions,entries,non_public_entries,\
        in_module_calls,in_package_calls,cross_package_calls,framework_calls"
    );
    env.modules.iter().for_each(|module| {
        let package = &env.packages[module.package];

        let struct_count = module.structs.len();
        let func_count = module.functions.len();
        let const_count = module.constants.len();

        let mut key_structs = 0usize;
        let mut store_structs = 0usize;
        let mut other_structs = 0usize;
        module.structs.iter().for_each(|struct_idx| {
            let struct_ = &env.structs[*struct_idx];
            if struct_.abilities.has_key() {
                store_structs += 1;
            } else if struct_.abilities.has_store() {
                key_structs += 1;
            } else {
                other_structs += 1;
            }
        });

        let mut public = 0usize;
        let mut entries = 0usize;
        let mut non_public_entries = 0usize;
        let mut in_module_calls = 0usize;
        let mut in_package_calls = 0usize;
        let mut cross_package_calls = 0usize;
        let mut framework_calls = 0usize;
        module.functions.iter().for_each(|func_idx| {
            let func = &env.functions[*func_idx];
            if func.visibility == Visibility::Public {
                public += 1;
                if func.is_entry {
                    entries += 1;
                }
            } else if func.is_entry {
                non_public_entries += 1;
            }

            if let Some(code) = func.code.as_ref() {
                code.code.iter().for_each(|bytecode| match bytecode {
                    Bytecode::Call(func_idx) | Bytecode::CallGeneric(func_idx, _) => {
                        let func = &env.functions[*func_idx];
                        let func_pkg_idx = func.package;
                        if env.framework.contains_key(&func_pkg_idx) {
                            framework_calls += 1;
                        } else if package.direct_dependencies.contains(&func_pkg_idx) {
                            cross_package_calls += 1;
                        } else if func.module == module.self_idx {
                            in_module_calls += 1;
                        } else {
                            in_package_calls += 1;
                        }
                    }
                    _ => (), // continue
                });
            };
        });

        let package = &env.packages[module.package];
        write_to!(
            buffer,
            "{},{},0x{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            // package,version,module,
            package.id,
            package.version,
            module.module_id,
            // dependencies,
            module.dependencies.len(),
            // structs,functions,constants,
            struct_count,
            func_count,
            const_count,
            // key_structs,store_structs,other_structs,
            key_structs,
            store_structs,
            other_structs,
            // public_functions,entries,non_public_entries,
            public,
            entries,
            non_public_entries,
            // in_module_calls,in_package_calls,cross_package_calls,framework_calls
            in_module_calls,
            in_package_calls,
            cross_package_calls,
            framework_calls,
        );
    });

    buffer.flush().unwrap();
}

/// Generate a `binary_modules.csv` file with compiled module information.
/// This is mostly dumping table sizes.
fn binary_modules(env: &GlobalEnv, output: &Path) {
    let file = &mut File::create(output.join("binary_modules.csv")).unwrap_or_else(|_| {
        panic!(
            "Unable to create file bynary_module.csv in {}",
            output.display()
        )
    });
    let buffer = &mut BufWriter::new(file);

    // columns:
    //   package                            package id the module belongs to
    //   version                            version of the package (starting from 1)
    //   module                             ModuleId as in the binary format
    //   module_handles                     number of module handles
    //   struct_handles                     number of struct handles
    //   function_handles                   number of function handles
    //   field_handles                      number of field handles
    //   struct_def_instantiations          number of struct instantiations
    //   function_instantiations            number of function instantiations
    //   field_instantiations               number of field instantiations
    //   signatures                         number of signatures
    //   identifiers                        number of identifiers
    //   address_identifiers                number of address identifiers
    //   constant_pool                      number of constants
    //   struct_defs                        number of struct definitions
    //   function_defs                      number of function definitions
    write_to!(
        buffer,
        "package,version,module,\
        module_handles,struct_handles,function_handles,field_handles,\
        struct_def_instantiations,function_instantiations,field_instantiations,\
        signatures,identifiers,address_identifiers,constant_pool,\
        struct_defs,function_defs"
    );
    env.modules.iter().for_each(|module| {
        if let Some(compiled_module) = module.module.as_ref() {
            let package = &env.packages[module.package];
            write_to!(
                buffer,
                "{},{},0x{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                // package,version,module,
                package.id,
                package.version,
                module.module_id,
                // module_handles,struct_handles,function_handles,field_handles,
                compiled_module.module_handles.len(),
                compiled_module.struct_handles.len(),
                compiled_module.function_handles.len(),
                compiled_module.field_handles.len(),
                // struct_def_instantiations,function_instantiations,field_instantiations,
                compiled_module.struct_def_instantiations.len(),
                compiled_module.function_instantiations.len(),
                compiled_module.field_instantiations.len(),
                // signatures,identifiers,address_identifiers,constant_pool,
                compiled_module.signatures.len(),
                compiled_module.identifiers.len(),
                compiled_module.address_identifiers.len(),
                compiled_module.constant_pool.len(),
                // struct_defs,function_defs
                compiled_module.struct_defs.len(),
                compiled_module.function_defs.len(),
            );
        }
    });

    buffer.flush().unwrap();
}

/// Generate a `functions.csv` file with function information.
fn functions(env: &GlobalEnv, output: &Path) {
    let file = &mut File::create(output.join("functions.csv")).unwrap_or_else(|_| {
        panic!(
            "Unable to create file functions.csv in {}",
            output.display()
        )
    });
    let buffer = &mut BufWriter::new(file);

    // columns:
    //   package                            package id the module belongs to
    //   version                            version of the package (starting from 1)
    //   module_address                     address of the module
    //   module                             module name
    //   function                           function name
    //   visibility                         function visibility
    //   entry                              true if the function is an entry point
    //   type_parameters                    number of type parameters
    //   parameters                         number of function arguments
    //   returns                            number of function return values
    //   instructions                       number of instructions in the function
    //   in_package_calls                   count of calls to functions in the same package
    //   cross_package_calls                count of calls to functions in other packages, except framework packages
    //   framework_calls                    count of calls to functions in the framework
    write_to!(
        buffer,
        "package,version,module_address,module,\
        function,visibility,entry,native,\
        type_parameters,parameters,returns,instructions,\
        in_package_calls,cross_package_calls,framework_calls",
    );
    env.functions.iter().for_each(|func| {
        let package = &env.packages[func.package];
        let module = &env.modules[func.module];
        let mut in_package_calls = 0usize;
        let mut cross_package_calls = 0usize;
        let mut framework_calls = 0usize;
        let code_len = func.code.as_ref().map_or(0, |code| {
            code.code.iter().for_each(|bytecode| match bytecode {
                Bytecode::Call(func_idx) | Bytecode::CallGeneric(func_idx, _) => {
                    let func = &env.functions[*func_idx];
                    let func_pkg_idx = func.package;
                    if env.framework.contains_key(&func_pkg_idx) {
                        framework_calls += 1;
                    } else if package.dependencies.contains(&func_pkg_idx) {
                        cross_package_calls += 1;
                    } else {
                        in_package_calls += 1;
                    }
                }
                _ => {}
            });
            code.code.len()
        });
        write_to!(
            buffer,
            "{},{},0x{},{},{},{:?},{},{},{},{},{},{},{},{},{}",
            // package, version, module_address, module,
            package.id,
            package.version,
            module.module_id.address(),
            env.module_name(module),
            // function, visibility, entry, native,
            env.function_name(func),
            func.visibility,
            func.is_entry,
            code_len == 0,
            // type_parameters, parameters, returns, instructions,
            func.type_parameters.len(),
            func.parameters.len(),
            func.returns.len(),
            code_len,
            // in_package_calls, cross_package_calls, framework_calls
            in_package_calls,
            cross_package_calls,
            framework_calls,
        );
    });

    buffer.flush().unwrap();
}

/// Generate a `structs.csv` file with type information.
fn structs(env: &GlobalEnv, output: &Path) {
    let file = &mut File::create(output.join("structs.csv"))
        .unwrap_or_else(|_| panic!("Unable to create file structs.csv in {}", output.display()));
    let buffer = &mut BufWriter::new(file);

    // columns:
    //   package                            package id the module belongs to
    //   version                            version of the package (starting from 1)
    //   module_address                     address of the module
    //   module                             module name
    //   struct                             struct name
    //   type_parameters                    number of type parameters
    //   abilities                          abilities of the struct as a u8 (see file_format.rs::AbilitySet
    //   fields                             number of fields
    write_to!(
        buffer,
        "package,version,module_address,module,struct,type_parameters,abilities,fields"
    );
    env.structs.iter().for_each(|struct_| {
        let package = &env.packages[struct_.package];
        let module = &env.modules[struct_.module];
        write_to!(
            buffer,
            "{}, {}, 0x{}, {}, {}, {}, {:04b}, {}",
            // "package, version, module_address, module,
            package.id,
            package.version,
            module.module_id.address(),
            env.module_name(module),
            // struct, type_parameters, abilities, fields
            env.struct_name(struct_),
            struct_.type_parameters.len(),
            struct_.abilities.into_u8(),
            struct_.fields.len(),
        );
    });

    buffer.flush().unwrap();
}
