// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod ast;
mod refinement;
mod structuring;

pub mod config;
pub mod pretty_printer;
pub mod testing;
pub mod translate;

use anyhow::anyhow;
use move_model_2::{
    compiled_model as CM,
    model::{self as M, Model},
    source_kind::SourceKind,
};

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

//--------------------------------------------------------------------------------------------------
// Output Generation for Decompilation from Compiled Modules
//--------------------------------------------------------------------------------------------------

/// Generate Move source code from a list of compiled Move module files (.mv)
/// and write the output to the specified directory.
/// The output directory will contain subdirectories for each package,
/// with the decompiled Move source files.
/// # Arguments
/// * `input_files` - A slice of PathBufs representing the input .mv files.
/// * `output` - A Path representing the output directory.
/// # Returns
/// * `anyhow::Result<Vec<Path>>` - A result containing a vector of paths to the generated files,
pub fn generate_from_files(input_files: &[PathBuf], output: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let module_bytes = input_files
        .iter()
        .map(|path| {
            let path = path.canonicalize().map_err(|e| {
                let path = path.display();
                anyhow!(format!("Failed to canonicalize path {path}: {e}"))
            })?;
            // read raw bytes from file
            std::fs::read(&path)
                .map_err(|e| anyhow!(format!("Failed to read file {}: {}", path.display(), e)))
                .map(|bytes| (path.clone(), bytes))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let modules = module_bytes
        .iter()
        .map(|(path, bytes)| {
            let path = path.display();
            move_binary_format::file_format::CompiledModule::deserialize_with_defaults(bytes)
                .map_err(|e| anyhow!(format!("Failed to deserialize module at {path}: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let model_config = M::ModelConfig {
        // During decompilation, we do not need to resolve all dependencies.
        allow_missing_dependencies: true,
    };
    let model = CM::Model::from_compiled_with_config(model_config, &BTreeMap::new(), modules);
    generate_from_model(model, output)
}

/// Generate Move source code from a model and write the output to the specified directory. The
/// output directory will contain subdirectories for each package, with the decompiled Move source
/// files.
/// # Arguments
/// * `input` - A Model representing the compiled Move modules.
/// * `output` - A Path representing the output directory.
/// # Returns
/// * `anyhow::Result<Vec<PathBuf>>` - A result containing a vector of paths to the generated files
pub fn generate_from_model<S: SourceKind>(
    input: Model<S>,
    output: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    let decompiled = crate::translate::model(input)?;

    let crate::ast::Decompiled { model, packages } = decompiled;

    println!("Packages\n----------------------------------");
    println!(
        "- {:#?}",
        model
            .packages()
            .map(|p| (p.name(), p.address()))
            .collect::<Vec<_>>()
    );

    let mut output_paths = vec![];

    println!("Modules\n----------------------------------");
    for pkg in packages {
        let name = pkg
            .name
            .map(|name| name.as_str().to_owned())
            .unwrap_or_else(|| format!("{}", pkg.address));

        // Ensure the package directory exists and is empty: output/pkg_name
        let pkg_dir = output.join(&name);
        std::fs::create_dir_all(&pkg_dir)?;

        let Some(model_pkg) = model.maybe_package(&pkg.address) else {
            anyhow::bail!("Package with address {} not found in model", pkg.address);
        };

        // Iterate without moving the map/vec
        for (module_name, module) in &pkg.modules {
            let path = pkg_dir.join(format!("{module_name}.move"));
            // If generate_output returns a Result, use `?`; otherwise drop it
            output_paths.push(generate_module(&model, model_pkg, &path, &name, module)?);
        }
    }

    Ok(output_paths)
}

fn generate_module<S: SourceKind>(
    model: &Model<S>,
    pkg: M::Package<'_, S>,
    path: &PathBuf,
    pkg_name: &str,
    module: &crate::ast::Module,
) -> anyhow::Result<PathBuf> {
    let Some(model_mod) = pkg.maybe_module(module.name) else {
        anyhow::bail!("Module {} not found in package {}", module.name, pkg_name);
    };

    let doc = pretty_printer::module(model, pkg_name, model_mod, module)?;

    let output = doc.render(100);
    println!("- {}", path.display());
    let _ = std::fs::remove_file(path); // ignore error if file does not exist
    std::fs::write(path, output)?;
    Ok(path.into())
}
