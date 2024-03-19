// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_docgen::DocgenOptions;
use move_package::{BuildConfig, ModelConfig};
use std::{fs, path::PathBuf};

/// Generate javadoc style documentation for Move packages
#[derive(Parser)]
#[clap(name = "docgen")]
pub struct Docgen {
    /// The level where we start sectioning. Often markdown sections are rendered with
    /// unnecessary large section fonts, setting this value high reduces the size
    #[clap(long = "section-level-start", value_name = "HEADER_LEVEL")]
    pub section_level_start: Option<usize>,
    /// Whether to exclude private functions in the generated docs
    #[clap(long = "exclude-private-fun")]
    pub exclude_private_fun: bool,
    /// Whether to exclude specifications in the generated docs
    #[clap(long = "exclude-specs")]
    pub exclude_specs: bool,
    /// Whether to put specifications in the same section as a declaration or put them all
    /// into an independent section
    #[clap(long = "independent-specs")]
    pub independent_specs: bool,
    /// Whether to exclude Move implementations
    #[clap(long = "exclude-impl")]
    pub exclude_impl: bool,
    /// Max depth to which sections are displayed in table-of-contents
    #[clap(long = "toc-depth", value_name = "DEPTH")]
    pub toc_depth: Option<usize>,
    /// Do not use collapsed sections (<details>) for impl and specs
    #[clap(long = "no-collapsed-sections")]
    pub no_collapsed_sections: bool,
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
    /// Whether to include dependency diagrams in the generated docs
    #[clap(long = "include-dep-diagrams")]
    pub include_dep_diagrams: bool,
    /// Whether to include call diagrams in the generated docs
    #[clap(long = "include-call-diagrams")]
    pub include_call_diagrams: bool,
    /// If this is being compiled relative to a different place where it will be stored (output directory)
    #[clap(long = "compile-relative-to-output-dir")]
    pub compile_relative_to_output_dir: bool,
}

impl Docgen {
    /// Calling the Docgen
    pub fn execute(self, path: Option<PathBuf>, config: BuildConfig) -> anyhow::Result<()> {
        let model = config.move_model_for_package(
            &reroot_path(path).unwrap(),
            ModelConfig {
                all_files_as_targets: false,
                target_filter: None,
            },
        )?;

        let mut options = DocgenOptions::default();

        if !self.template.is_empty() {
            options.root_doc_templates = self.template;
        }
        if self.section_level_start.is_some() {
            options.section_level_start = self.section_level_start.unwrap();
        }
        if self.exclude_private_fun {
            options.include_private_fun = false;
        }
        if self.exclude_specs {
            options.include_specs = false;
        }
        if self.independent_specs {
            options.specs_inlined = false;
        }
        if self.exclude_impl {
            options.include_impl = false;
        }
        if self.toc_depth.is_some() {
            options.toc_depth = self.toc_depth.unwrap();
        }
        if self.no_collapsed_sections {
            options.collapsed_sections = false;
        }
        if self.output_directory.is_some() {
            options.output_directory = self.output_directory.unwrap();
        }
        if self.references_file.is_some() {
            options.references_file = self.references_file;
        }
        if self.compile_relative_to_output_dir {
            options.compile_relative_to_output_dir = true;
        }

        // We are using the full namespace, since we already use `Docgen` here.
        // Docgen is the most suitable name for both: this Docgen subcommand,
        // and the actual move_docgen::Docgen.
        let generator = move_docgen::Docgen::new(&model, &options);

        for (file, content) in generator.gen() {
            let path = PathBuf::from(&file);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path.as_path(), content)?;
            println!("Generated {:?}", path);
        }

        anyhow::ensure!(
            !model.has_errors(),
            "Errors encountered while generating documentation!"
        );

        println!("\nDocumentation generation successful!");
        Ok(())
    }
}
