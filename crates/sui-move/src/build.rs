// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use codespan_reporting::{diagnostic::Severity, term::termcolor::Buffer};
use move_binary_format::file_format::Visibility;
use move_cli::base::{self, build};
use move_package::{BuildConfig as MoveBuildConfig, ModelConfig};
use move_stackless_bytecode::{
    stackless_bytecode::{Bytecode, Operation},
    stackless_bytecode_generator::StacklessBytecodeGenerator,
    stackless_control_flow_graph::StacklessControlFlowGraph,
};
use serde_json::json;
use std::{collections::BTreeSet, fs, path::PathBuf};
use sui_move_build::{check_invalid_dependencies, check_unpublished_dependencies, BuildConfig};
use sui_types::base_types::ObjectID;

use crate::linters::self_transfer::SelfTransferAnalysis;

const LAYOUTS_DIR: &str = "layouts";
const STRUCT_LAYOUTS_FILENAME: &str = "struct_layouts.yaml";

#[derive(Parser)]
pub struct Build {
    #[clap(flatten)]
    pub build: build::Build,
    /// Include the contents of packages in dependencies that haven't been published (only relevant
    /// when dumping bytecode as base64)
    #[clap(long, global = true)]
    pub with_unpublished_dependencies: bool,
    /// Use the legacy digest calculation algorithm
    #[clap(long)]
    legacy_digest: bool,
    /// Whether we are printing in base64.
    #[clap(long, global = true)]
    pub dump_bytecode_as_base64: bool,
    /// If true, generate struct layout schemas for
    /// all struct types passed into `entry` functions declared by modules in this package
    /// These layout schemas can be consumed by clients (e.g.,
    /// the TypeScript SDK) to enable serialization/deserialization of transaction arguments
    /// and events.
    #[clap(long, global = true)]
    pub generate_struct_layouts: bool,
    /// If `true`, disable all linters
    #[clap(long, global = true)]
    pub no_lint: bool,
    /// If `true`, do not print linter output in color
    #[clap(long, global = true)]
    pub no_color: bool,
}

impl Build {
    pub fn execute(
        &self,
        path: Option<PathBuf>,
        build_config: MoveBuildConfig,
    ) -> anyhow::Result<()> {
        let rerooted_path = base::reroot_path(path.clone())?;
        let build_config = resolve_lock_file_path(build_config, path)?;
        Self::execute_internal(
            rerooted_path,
            build_config,
            self.with_unpublished_dependencies,
            self.legacy_digest,
            self.dump_bytecode_as_base64,
            self.generate_struct_layouts,
            !self.no_lint,
            !self.no_color,
        )
    }

    pub fn execute_internal(
        rerooted_path: PathBuf,
        config: MoveBuildConfig,
        with_unpublished_deps: bool,
        legacy_digest: bool,
        dump_bytecode_as_base64: bool,
        generate_struct_layouts: bool,
        lint: bool,
        color: bool,
    ) -> anyhow::Result<()> {
        let build_config = BuildConfig {
            config,
            run_bytecode_verifier: true,
            print_diags_to_stderr: true,
        };
        let pkg = build_config.clone().build(rerooted_path.clone())?;
        if dump_bytecode_as_base64 {
            check_invalid_dependencies(&pkg.dependency_ids.invalid)?;
            if !with_unpublished_deps {
                check_unpublished_dependencies(&pkg.dependency_ids.unpublished)?;
            }

            let package_dependencies = pkg.get_package_dependencies_hex();
            println!(
                "{}",
                json!({
                    "modules": pkg.get_package_base64(with_unpublished_deps),
                    "dependencies": json!(package_dependencies),
                    "digest": pkg.get_package_digest(with_unpublished_deps, !legacy_digest),
                })
            )
        }

        if generate_struct_layouts {
            let layout_str = serde_yaml::to_string(&pkg.generate_struct_layouts()).unwrap();
            // store under <package_path>/build/<package_name>/layouts/struct_layouts.yaml
            let mut layout_filename = pkg.path;
            layout_filename.push("build");
            layout_filename.push(pkg.package.compiled_package_info.package_name.as_str());
            layout_filename.push(LAYOUTS_DIR);
            layout_filename.push(STRUCT_LAYOUTS_FILENAME);
            fs::write(layout_filename, layout_str)?
        }

        if lint {
            let env = build_config.config.move_model_for_package(
                &rerooted_path,
                ModelConfig {
                    all_files_as_targets: false,
                    target_filter: None,
                },
            )?;
            let published_addr = pkg.published_at.unwrap_or(ObjectID::ZERO);
            // check for unused functions
            for module_env in env.get_modules() {
                if ObjectID::from_address(*module_env.self_address()) != published_addr {
                    // do not look at dependencies
                    continue;
                }
                for func_env in module_env.get_functions() {
                    // module inits are supposed to be unused
                    if func_env.visibility() != Visibility::Public
                        && func_env.get_name_str() != "init"
                    {
                        if func_env.get_called_functions().is_empty() {
                            env.diag(Severity::Error, &func_env.get_loc(), &format!("Unused private or `friend` function {}. This function should be called or deleted", func_env.get_full_name_str()))
                        }
                    }
                }
            }

            for module_env in env.get_modules() {
                if ObjectID::from_address(*module_env.self_address()) != published_addr {
                    // do not lint dependencies
                    continue;
                }
                let mut packed_types = BTreeSet::new();
                for func_env in module_env.get_functions() {
                    if func_env.is_native() {
                        // do not lint on native functions
                        continue;
                    }
                    let generator = StacklessBytecodeGenerator::new(&func_env);
                    let fun_data = generator.generate_function();
                    for instr in &fun_data.code {
                        match instr {
                            Bytecode::Call(_, _, Operation::Pack(_, sid, _), ..) => {
                                packed_types.insert(*sid);
                            }
                            _ => (),
                        }
                    }
                    let cfg = StacklessControlFlowGraph::new_forward(&fun_data.code);
                    // warn on calls of `public_transfer(.., tx_context::sender())`
                    SelfTransferAnalysis::analyze(&func_env, &fun_data, &cfg);
                    // calls to additional linters should go here
                }
                // check for unused types
                for t in module_env.get_structs() {
                    // TODO: better check for one-time witness. for now, we just use all caps as a proxy. this will catch all OTW's, but will miss some unused structs
                    if !packed_types.contains(&t.get_id())
                        && t.get_name_string() != t.get_name_string().to_ascii_uppercase()
                    {
                        env.diag(
                            Severity::Error,
                            &t.get_loc(),
                            &format!(
                                "Unused struct type {}. This type should be used or deleted",
                                t.get_full_name_str()
                            ),
                        )
                    }
                }
            }
            let mut error_writer = if color {
                Buffer::ansi()
            } else {
                Buffer::no_color()
            };
            env.report_diag(&mut error_writer, Severity::Warning);
            println!("{}", String::from_utf8_lossy(&error_writer.into_inner()));
        }

        Ok(())
    }
}

/// Resolve Move.lock file path in package directory (where Move.toml is).
pub fn resolve_lock_file_path(
    mut build_config: MoveBuildConfig,
    package_path: Option<PathBuf>,
) -> Result<MoveBuildConfig, anyhow::Error> {
    if build_config.lock_file.is_none() {
        let package_root = base::reroot_path(package_path)?;
        let lock_file_path = package_root.join("Move.lock");
        build_config.lock_file = Some(lock_file_path);
    }
    Ok(build_config)
}
