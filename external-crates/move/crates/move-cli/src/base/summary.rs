// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::base::reroot_path;
use clap::*;
use move_binary_format::CompiledModule;
use move_command_line_common::files::{MOVE_COMPILED_EXTENSION, extension_equals, find_filenames};
use move_model_2 as M2;
use move_package::BuildConfig;
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

const COMMAND_NAME: &str = "summary";
const DEFAULT_OUTPUT_DIRECTORY: &str = "package_summaries";
const METADATATA_FILENAME: &str = "root_package_metadata";
const ADDRESS_MAPPING_FILENAME: &str = "address_mapping";

const YAML_EXT: &str = "yaml";
const JSON_EXT: &str = "json";

/// Generate a serialized summary of a Move package (e.g., functions, structs, annotations, etc.)
#[derive(Parser)]
#[clap(name = COMMAND_NAME)]
pub struct Summary {
    /// The output format the summary should be generated in.
    #[arg(value_enum, long, short, default_value_t = SummaryOutputFormat::Json)]
    pub output_format: SummaryOutputFormat,
    /// Directory that all generated summaries should be nested under.
    #[clap(long = "output-directory", value_name = "PATH", default_value = DEFAULT_OUTPUT_DIRECTORY)]
    pub output_directory: String,
    /// Whether we are generating a summary for a package or for a directory of bytecode modules.
    /// All `.mv` files under the path supplied (or current directory if none supplied) will be summarized.
    #[clap(long = "bytecode")]
    pub bytecode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SummaryOutputFormat {
    #[clap(name = JSON_EXT)]
    Json,
    #[clap(name = YAML_EXT)]
    Yaml,
}

impl Summary {
    pub fn execute<T: Serialize + ?Sized>(
        self,
        path: Option<&Path>,
        config: BuildConfig,
        additional_metadata: Option<&T>,
    ) -> anyhow::Result<()> {
        let model_source;
        let model_compiled;

        let summary = if self.bytecode {
            let input_path = path.unwrap_or_else(|| Path::new("."));
            let bytecode_files = find_filenames(&[input_path], |path| {
                extension_equals(path, MOVE_COMPILED_EXTENSION)
            })?;

            let mut modules = Vec::new();
            for bytecode_file in &bytecode_files {
                let bytes = std::fs::read(bytecode_file)?;
                modules.push(CompiledModule::deserialize_with_defaults(&bytes)?);
            }

            model_compiled = M2::compiled_model::Model::from_compiled(&BTreeMap::new(), modules);
            model_compiled.summary()
        } else {
            model_source = config
                .move_model_for_package(&reroot_path(path).unwrap(), &mut std::io::stdout())?;
            model_source.summary()
        };

        self.output_summaries(summary, additional_metadata)?;

        println!(
            "\nSummary generation successful. Summaries stored in '{}'",
            self.output_directory
        );
        Ok(())
    }

    fn output_summaries<T: Serialize + ?Sized>(
        &self,
        summaries: &M2::summary::Packages,
        additional_metadata: Option<&T>,
    ) -> anyhow::Result<()> {
        let output_dir = Path::new(&self.output_directory);
        std::fs::create_dir_all(output_dir)?;

        let mut address_mapping = BTreeMap::new();

        for (package_addr, package_summary) in summaries.packages.iter() {
            let package_name = package_summary
                .name
                .map(|s| s.to_string())
                .unwrap_or_else(|| package_addr.to_canonical_string(/* with_prefix */ true));

            address_mapping.insert(
                package_addr.to_canonical_string(/* with_prefix */ true),
                package_name.clone(),
            );

            let package_dir = output_dir.join(package_name);
            std::fs::create_dir_all(&package_dir)?;

            for (module_name, module) in &package_summary.modules {
                let module_summary_file = package_dir.join(module_name.to_string());
                self.serialize_to_file(module, &module_summary_file)?;
                    }
                    }

        self.serialize_to_file(&address_mapping, &output_dir.join(ADDRESS_MAPPING_FILENAME))?;

        if let Some(additional_metadata) = additional_metadata {
            let metadata_file = output_dir.join(METADATATA_FILENAME);
            self.serialize_to_file(additional_metadata, &metadata_file)?;
        }

        Ok(())
    }

    pub fn serialize_to_file<T: Serialize + ?Sized>(
        &self,
        serializable_data: &T,
        path: &Path,
    ) -> anyhow::Result<()> {
        match self.output_format {
            SummaryOutputFormat::Json => std::fs::write(
                path.with_extension(JSON_EXT),
                serde_json::to_string_pretty(serializable_data)?,
            ),
            SummaryOutputFormat::Yaml => std::fs::write(
                path.with_extension(YAML_EXT),
                serde_yaml::to_string(serializable_data)?,
            ),
        }?;
        Ok(())
    }
}
