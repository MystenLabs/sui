// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::utils::get_loc;
use codespan_reporting::{diagnostic::Severity, files::SimpleFiles};
use lsp_types::{Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, Range};
use move_command_line_common::files::FileHash;
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, HashMap};
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
    files: &SimpleFiles<Symbol, String>,
    file_id_mapping: &HashMap<FileHash, usize>,
    file_name_mapping: &BTreeMap<FileHash, Symbol>,
) -> BTreeMap<Symbol, Vec<Diagnostic>> {
    let mut lsp_diagnostics = BTreeMap::new();
    for (s, _, (loc, msg), labels, _) in diagnostics {
        let fpath = file_name_mapping.get(&loc.file_hash()).unwrap();
        if let Some(start) = get_loc(&loc.file_hash(), loc.start(), files, file_id_mapping) {
            if let Some(end) = get_loc(&loc.file_hash(), loc.end(), files, file_id_mapping) {
                let range = Range::new(start, end);
                let related_info_opt = if labels.is_empty() {
                    None
                } else {
                    Some(
                        labels
                            .iter()
                            .filter_map(|(lloc, lmsg)| {
                                let lstart = get_loc(
                                    &lloc.file_hash(),
                                    lloc.start(),
                                    files,
                                    file_id_mapping,
                                )?;
                                let lend =
                                    get_loc(&lloc.file_hash(), lloc.end(), files, file_id_mapping)?;
                                let lpath = file_name_mapping.get(&lloc.file_hash()).unwrap();
                                let lpos = Location::new(
                                    Url::from_file_path(lpath.as_str()).unwrap(),
                                    Range::new(lstart, lend),
                                );
                                Some(DiagnosticRelatedInformation {
                                    location: lpos,
                                    message: lmsg.to_string(),
                                })
                            })
                            .collect(),
                    )
                };
                lsp_diagnostics
                    .entry(*fpath)
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
    file_name_mapping: &BTreeMap<FileHash, Symbol>,
) -> BTreeMap<Symbol, Vec<Diagnostic>> {
    let mut lsp_diagnostics = BTreeMap::new();
    for n in file_name_mapping.values() {
        lsp_diagnostics.insert(*n, vec![]);
    }
    lsp_diagnostics
}

/// Converts diagnostic severity level from the codespan format to the format understood by the
/// language server.
fn severity(s: Severity) -> DiagnosticSeverity {
    match s {
        Severity::Bug => DiagnosticSeverity::Error,
        Severity::Error => DiagnosticSeverity::Error,
        Severity::Warning => DiagnosticSeverity::Warning,
        Severity::Note => DiagnosticSeverity::Information,
        Severity::Help => DiagnosticSeverity::Hint,
    }
}
