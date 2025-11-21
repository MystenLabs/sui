// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::*;

use crate::base::reroot_path;
use move_docgen::{DocgenFlags, DocgenOptions};
use move_package_alt::flavor::MoveFlavor;
use move_package_alt_compilation::{build_config::BuildConfig, find_env};

/// Generate Rust style documentation for Move packages
#[derive(Parser)]
#[clap(name = "docgen")]
pub struct Docgen {
    #[clap(flatten)]
    pub flags: DocgenFlags,
    /// In which directory to store output
    #[clap(long = "output-directory", value_name = "PATH")]
    pub output_directory: Option<String>,
    /// A template for documentation generation. Can be multiple
    #[clap(long = "template", short = 't', value_name = "FILE")]
    pub template: Vec<String>,
    /// An optional file containing reference definitions. The content of this file will
    /// be added to each generated markdown doc
    #[clap(long = "references-file", value_name = "FILE")]
    pub references_file: Option<String>,
    /// If this is being compiled relative to a different place where it will be stored (output directory)
    #[clap(long = "compile-relative-to-output-dir")]
    pub compile_relative_to_output_dir: bool,
}

impl Docgen {
    /// Calling the Docgen
    pub async fn execute<F: MoveFlavor>(
        self,
        path: Option<&Path>,
        config: BuildConfig,
    ) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        let env = find_env::<F>(&rerooted_path, &config)?;
        let model = config
            .move_model_from_path::<F, _>(&rerooted_path, env, &mut std::io::stdout())
            .await?;

        let mut options = DocgenOptions {
            flags: self.flags,
            ..DocgenOptions::default()
        };

        if !self.template.is_empty() {
            options.root_doc_templates = self.template;
        }
        if let Some(dir) = self.output_directory {
            options.output_directory = dir;
        }
        options.references_file = self.references_file;
        options.compile_relative_to_output_dir = self.compile_relative_to_output_dir;

        let docgen = move_docgen::Docgen::new(&model, &options);

        for (file, content) in docgen.generate(&model)? {
            let path = PathBuf::from(&file);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path.as_path(), content)?;
            println!("Generated {:?}", path);
        }

        println!("\nDocumentation generation successful!");
        Ok(())
    }
}
