// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use move_command_line_common::files::FileHash;
use move_compiler::shared::ide as CI;
use move_ir_types::location::Loc;

/// Compiler information used during symbolication analysis.
/// This is cached and used during typing analysis.
#[derive(Default, Debug, Clone)]
pub struct CompilerAnalysisInfo {
    /// Macro call information
    pub macro_info: BTreeMap<Loc, CI::MacroCallInfo>,
    /// Expanded lambda expressions
    pub expanded_lambdas: BTreeSet<Loc>,
    /// Ellipsis-generated binders (to filter from IDE)
    pub ellipsis_binders: BTreeSet<Loc>,
    /// Original string values recorded during parsing
    pub string_values: BTreeMap<Loc, String>,
}

/// Compiler information used for IDE autocomplete features.
/// This is NOT cached, only kept in Symbols for IDE requests.
#[derive(Default, Debug, Clone)]
pub struct CompilerAutocompleteInfo {
    /// Dot autocomplete information (obj.method)
    pub dot_autocomplete_info: BTreeMap<FileHash, BTreeMap<Loc, CI::DotAutocompleteInfo>>,
    /// Path autocomplete information (module::path)
    pub path_autocomplete_info: BTreeMap<Loc, CI::AliasAutocompleteInfo>,
}

impl CompilerAnalysisInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_macro_info(&self, loc: &Loc) -> Option<&CI::MacroCallInfo> {
        self.macro_info.get(loc)
    }

    pub fn is_expanded_lambda(&self, loc: &Loc) -> bool {
        self.expanded_lambdas.contains(loc)
    }
}

impl CompilerAutocompleteInfo {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get dot autocomplete information (for obj.field completions)
    pub fn get_dot_autocomplete_info(
        &self,
        fhash: FileHash,
        loc: &Loc,
    ) -> Option<&CI::DotAutocompleteInfo> {
        self.dot_autocomplete_info.get(&fhash).and_then(|a| {
            a.iter().find_map(|(aloc, ainfo)| {
                if aloc.contains(loc) {
                    Some(ainfo)
                } else {
                    None
                }
            })
        })
    }

    /// Get path autocomplete information (for module::path completions)
    pub fn get_path_autocomplete_info(&self, loc: &Loc) -> Option<&CI::AliasAutocompleteInfo> {
        self.path_autocomplete_info.get(loc)
    }
}

/// Process compiler IDE annotations into analysis and autocomplete info.
/// Returns (analysis_info, autocomplete_info)
pub fn process_ide_annotations(
    annotations: impl IntoIterator<Item = (Loc, CI::IDEAnnotation)>,
) -> (CompilerAnalysisInfo, CompilerAutocompleteInfo) {
    let mut analysis = CompilerAnalysisInfo::default();
    let mut autocomplete = CompilerAutocompleteInfo::default();

    for (loc, entry) in annotations {
        match entry {
            CI::IDEAnnotation::MacroCallInfo(info) => {
                analysis.macro_info.insert(loc, *info);
            }
            CI::IDEAnnotation::ExpandedLambda => {
                analysis.expanded_lambdas.insert(loc);
            }
            CI::IDEAnnotation::DotAutocompleteInfo(info) => {
                autocomplete
                    .dot_autocomplete_info
                    .entry(loc.file_hash())
                    .or_default()
                    .insert(loc, *info);
            }
            CI::IDEAnnotation::PathAutocompleteInfo(info) => {
                autocomplete.path_autocomplete_info.insert(loc, *info);
            }
            CI::IDEAnnotation::EllipsisMatchEntries(_) => {
                analysis.ellipsis_binders.insert(loc);
            }
            CI::IDEAnnotation::MissingMatchArms(_) => {
                // TODO: Not much to do with this yet.
            }
            CI::IDEAnnotation::StringValue(string) => {
                analysis.string_values.insert(loc, *string);
            }
        }
    }

    (analysis, autocomplete)
}
