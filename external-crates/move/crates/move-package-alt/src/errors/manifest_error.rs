// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{ops::Range, path::PathBuf};

use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFiles,
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};
use thiserror::Error;

#[derive(Error, Debug)]
#[error("Invalid manifest: {kind}")]
pub struct ManifestError {
    pub kind: ManifestErrorKind,
    pub span: Option<Range<usize>>,
    pub path: PathBuf,
    pub src: String,
}

#[derive(Error, Debug)]
pub enum ManifestErrorKind {
    #[error("package name cannot be empty")]
    EmptyPackageName,
    #[error("unsupported edition '{edition}', expected one of '{valid}'")]
    InvalidEdition { edition: String, valid: String },
}

impl ManifestError {
    /// Convert this error into a codespan Diagnostic
    pub fn to_diagnostic(&self) -> Diagnostic<usize> {
        let (file_id, span) = self.span_info();
        Diagnostic::error()
            .with_message(self.kind.to_string())
            .with_labels(vec![Label::primary(file_id, span.unwrap_or_default())])
    }

    /// Get the file ID and span for this error
    fn span_info(&self) -> (usize, Option<Range<usize>>) {
        // In a real implementation, we'd want to cache the SimpleFiles instance
        // and reuse it across multiple errors
        let mut files = SimpleFiles::new();
        let file_id = files.add(self.path.display().to_string(), self.src.clone());
        (file_id, self.span.clone())
    }

    /// Emit this error to stderr
    pub fn emit(&self) -> Result<(), codespan_reporting::files::Error> {
        let mut files = SimpleFiles::new();
        let file_id = files.add(self.path.display().to_string(), self.src.clone());

        let writer = StandardStream::stderr(ColorChoice::Always);
        let config = term::Config {
            display_style: term::DisplayStyle::Rich,
            chars: term::Chars::ascii(),
            ..Default::default()
        };

        let diagnostic = self.to_diagnostic();
        let e = term::emit(&mut writer.lock(), &config, &files, &diagnostic);
        e
    }
}
