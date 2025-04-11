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

use crate::package::PackageName;

use super::FileHandle;

#[derive(Error, Debug)]
#[error("Invalid lockfile: {kind}")]
pub struct LockfileError {
    pub kind: LockfileErrorKind,
    pub span: Option<Range<usize>>,
    pub handle: FileHandle,
}

#[derive(Error, Debug)]
pub enum LockfileErrorKind {
    #[error(
        "Move.lock and Move.{env_name}.lock both contain publication information for {env_name}"
    )]
    SamePublicationInfo { env_name: String },
}

impl LockfileError {
    /// Convert this error into a codespan Diagnostic
    pub fn to_diagnostic(&self) -> Diagnostic<usize> {
        let (file_id, span) = self.span_info();
        Diagnostic::error()
            .with_message(self.kind.to_string())
            .with_labels(vec![Label::primary(file_id, span.unwrap_or_default())])
    }

    /// Get the file ID and span for this error
    fn span_info(&self) -> (usize, Option<Range<usize>>) {
        let mut files = SimpleFiles::new();
        let file_id = files.add(self.handle.path().to_string_lossy(), self.handle.source());
        (file_id, self.span.clone())
    }

    /// Emit this error to stderr
    pub fn emit(&self) -> Result<(), codespan_reporting::files::Error> {
        let mut files = SimpleFiles::new();
        let file_id = files.add(self.handle.path().to_string_lossy(), self.handle.source());

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
