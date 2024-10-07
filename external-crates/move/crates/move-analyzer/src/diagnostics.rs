// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::utils::{loc_end_to_lsp_position_opt, loc_start_to_lsp_position_opt};
use codespan_reporting::diagnostic::Severity;
use lsp_types::{Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, Range};
use move_command_line_common::files::FileHash;
use move_compiler::shared::files::MappedFiles;
use move_ir_types::location::Loc;
use std::{collections::BTreeMap, path::PathBuf};
use url::Url;

/// Converts diagnostics from the codespan format to the format understood by the language server.
pub fn lsp_diagnostics(
    diagnostics: &Vec<(
        codespan_reporting::diagnostic::Severity,
        &'static str,
        (Loc, String),
        Vec<(Loc, String)>,
        Vec<String>,
    )>,
    files: &MappedFiles,
) -> BTreeMap<PathBuf, Vec<Diagnostic>> {
    let mut lsp_diagnostics = BTreeMap::new();
    for (s, _, (loc, msg), labels, notes) in diagnostics {
        let fpath = files.file_path(&loc.file_hash());
        if let Some(start) = loc_start_to_lsp_position_opt(files, loc) {
            if let Some(end) = loc_end_to_lsp_position_opt(files, loc) {
                let range = Range::new(start, end);
                let related_info_opt = if labels.is_empty() && notes.is_empty() {
                    None
                } else {
                    Some(
                        labels
                            .iter()
                            .filter_map(|(lloc, lmsg)| {
                                let lstart = loc_start_to_lsp_position_opt(files, lloc)?;
                                let lend = loc_end_to_lsp_position_opt(files, lloc)?;
                                let lpath = files.file_path(&lloc.file_hash());
                                let lpos = Location::new(
                                    Url::from_file_path(lpath).unwrap(),
                                    Range::new(lstart, lend),
                                );
                                Some(DiagnosticRelatedInformation {
                                    location: lpos,
                                    message: lmsg.to_string(),
                                })
                            })
                            .chain(notes.iter().map(|note| {
                                // for notes use the same location as for the main message
                                let fpath = files.file_path(&loc.file_hash());
                                let fpos =
                                    Location::new(Url::from_file_path(fpath).unwrap(), range);
                                DiagnosticRelatedInformation {
                                    location: fpos,
                                    message: format!("Note: {note}"),
                                }
                            }))
                            .collect(),
                    )
                };
                lsp_diagnostics
                    .entry(fpath.to_path_buf())
                    .or_insert_with(Vec::new)
                    .push(Diagnostic::new(
                        range,
                        Some(severity(*s)),
                        None,
                        None,
                        msg.to_string(),
                        related_info_opt,
                        None,
                    ));
            }
        }
    }
    lsp_diagnostics
}

/// Produces empty diagnostics in the format understood by the language server for all files that
/// the language server is aware of.
pub fn lsp_empty_diagnostics(
    file_name_mapping: &BTreeMap<FileHash, PathBuf>,
) -> BTreeMap<PathBuf, Vec<Diagnostic>> {
    let mut lsp_diagnostics = BTreeMap::new();
    for n in file_name_mapping.values() {
        lsp_diagnostics.insert(n.to_path_buf(), vec![]);
    }
    lsp_diagnostics
}

/// Converts diagnostic severity level from the codespan format to the format understood by the
/// language server.
fn severity(s: Severity) -> DiagnosticSeverity {
    match s {
        Severity::Bug => DiagnosticSeverity::ERROR,
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Note => DiagnosticSeverity::INFORMATION,
        Severity::Help => DiagnosticSeverity::HINT,
    }
}
