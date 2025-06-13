// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_package_alt_compilation::build_config::BuildConfig as MoveBuildConfig;
use std::path::Path;
use sui_move_build::BuildConfig;

// const LAYOUTS_DIR: &str = "layouts";
// const STRUCT_LAYOUTS_FILENAME: &str = "struct_layouts.yaml";

#[derive(Parser)]
#[group(id = "sui-move-build")]
pub struct Build {
    #[clap(long, global = true)]
    pub dump_bytecode_as_base64: bool,
    /// [Mainly for testing, not recommended for production]
    /// Don't specialize the package to the active chain when dumping bytecode as Base64. This
    /// allows building to proceed without a network connection or active environment, but it
    /// will not be able to automatically determine the addresses of its dependencies.
    #[clap(long, global = true, requires = "dump_bytecode_as_base64")]
    pub ignore_chain: bool,
    /// If true, generate struct layout schemas for
    /// all struct types passed into `entry` functions declared by modules in this package
    /// These layout schemas can be consumed by clients (e.g.,
    /// the TypeScript SDK) to enable serialization/deserialization of transaction arguments
    /// and events.
    #[clap(long, global = true)]
    pub generate_struct_layouts: bool,
    /// The chain ID, if resolved. Required when the dump_bytecode_as_base64 is true,
    /// for automated address management, where package addresses are resolved for the
    /// respective chain in the Move.lock file.
    #[clap(skip)]
    pub chain_id: Option<String>,
}

impl Build {
    pub fn execute(
        &self,
        path: Option<&Path>,
        build_config: MoveBuildConfig,
    ) -> anyhow::Result<()> {
        let path = path.unwrap_or(Path::new("."));
        Self::execute_internal(
            path,
            build_config,
            self.generate_struct_layouts,
            self.chain_id.clone(),
        )
    }

    pub fn execute_internal(
        path: &Path,
        config: MoveBuildConfig,
        _generate_struct_layouts: bool,
        chain_id: Option<String>,
    ) -> anyhow::Result<()> {
        let _ = BuildConfig {
            config,
            run_bytecode_verifier: true,
            print_diags_to_stderr: true,
            chain_id,
        }
        .build(path)?;

        // if generate_struct_layouts {
        //     let layout_str = serde_yaml::to_string(&pkg.generate_struct_layouts()).unwrap();
        //     // store under <package_path>/build/<package_name>/layouts/struct_layouts.yaml
        //     let dir_name = rerooted_path
        //         .join("build")
        //         .join(pkg.package.compiled_package_info.package_name.as_str())
        //         .join(LAYOUTS_DIR);
        //     let layout_filename = dir_name.join(STRUCT_LAYOUTS_FILENAME);
        //     fs::create_dir_all(dir_name)?;
        //     fs::write(layout_filename, layout_str)?
        // }
        //
        // pkg.package
        //     .compiled_package_info
        //     .build_flags
        //     .update_lock_file_toolchain_version(rerooted_path, env!("CARGO_PKG_VERSION").into())?;
        //
        Ok(())
    }
}
