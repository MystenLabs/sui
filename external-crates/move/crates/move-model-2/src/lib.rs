// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[macro_use(sp)]
extern crate move_ir_types;

use codespan_reporting::term::termcolor::{StandardStream, WriteColor};
use move_compiler::{
    self,
    diagnostics::{env_color, output_diagnostics, Diagnostics, FilesSourceText},
    shared::PackagePaths,
    Compiler, PASS_TYPING,
};
use move_symbol_pool::Symbol;
use vfs::VfsPath;

pub mod code_writer;
pub mod display;
pub mod model;

pub use model::*;

// =================================================================================================
// Entry Point

/// A constructor for the `Model` that compiles the program from source
pub struct ModelCompiler {
    compiler: Option<Compiler>, // an option for in-place updates
    /// Output buffer for `Diagnostic` errors. Defaults to stderr.
    diag_buffer: Box<dyn WriteColor>,
}

impl ModelCompiler {
    pub fn from_package_paths<Paths: Into<Symbol>, NamedAddress: Into<Symbol>>(
        vfs_root: Option<VfsPath>,
        targets: Vec<PackagePaths<Paths, NamedAddress>>,
        deps: Vec<PackagePaths<Paths, NamedAddress>>,
    ) -> anyhow::Result<Self> {
        let color_choice = env_color();
        Ok(Self {
            compiler: Some(Compiler::from_package_paths(vfs_root, targets, deps)?),
            diag_buffer: Box::new(StandardStream::stderr(color_choice)),
        })
    }

    /// Modify the compiler for the builder. Useful for setting compiler flags and other settings
    pub fn modify_compiler(&mut self, f: impl FnOnce(Compiler) -> Compiler) {
        self.compiler = Some(f(self.compiler.take().unwrap()));
    }

    pub fn build(
        self,
    ) -> anyhow::Result<(FilesSourceText, Result<(Model, Diagnostics), Diagnostics>)> {
        let (files, _diag_buffer, res) = self.build_()?;
        Ok((files, res))
    }

    pub fn build_and_report(self) -> anyhow::Result<Model> {
        let (files, mut diag_buffer, res) = self.build_()?;
        let model = match res {
            Ok((model, warnings)) => {
                if !warnings.is_empty() {
                    output_diagnostics(&mut diag_buffer, &files, warnings)
                }
                model
            }
            Err(diags) => {
                output_diagnostics(&mut diag_buffer, &files, diags);
                std::process::exit(1)
            }
        };
        Ok(model)
    }

    fn build_(
        self,
    ) -> anyhow::Result<(
        FilesSourceText,
        Box<dyn WriteColor>,
        Result<(Model, Diagnostics), Diagnostics>,
    )> {
        let Self {
            compiler,
            diag_buffer,
        } = self;
        let compiler = compiler.unwrap();
        let (files, res) = compiler.run::<PASS_TYPING>()?;
        let (_comments, compiler) = match res {
            Ok((comments, compiler)) => (comments, compiler),
            Err((_, diags)) => return Ok((files, diag_buffer, Err(diags))),
        };
        let (compiler, typed_prog) = compiler.into_ast();
        let info = typed_prog.info.clone();
        let (compiled_units, warnings) = match compiler.at_typing(typed_prog).build() {
            Ok((compiled_units, warnings)) => (compiled_units, warnings),
            Err((_, diags)) => return Ok((files, diag_buffer, Err(diags))),
        };
        let model = Model::new(files.clone(), info, compiled_units)?;
        Ok((files, diag_buffer, Ok((model, warnings))))
    }
}
