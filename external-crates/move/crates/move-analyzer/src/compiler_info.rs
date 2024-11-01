// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use move_command_line_common::files::FileHash;
use move_compiler::shared::ide as CI;
use move_ir_types::location::Loc;

#[derive(Default, Debug, Clone)]
pub struct CompilerInfo {
    pub macro_info: BTreeMap<Loc, CI::MacroCallInfo>,
    pub expanded_lambdas: BTreeSet<Loc>,
    pub dot_autocomplete_info: BTreeMap<FileHash, BTreeMap<Loc, CI::DotAutocompleteInfo>>,
    pub path_autocomplete_info: BTreeMap<Loc, CI::AliasAutocompleteInfo>,
    /// Locations of binders in enum variants that are expanded from an ellipsis (and should
    /// not be displayed in any way by the IDE)
    pub ellipsis_binders: BTreeSet<Loc>,
    /// Locations of guard expressions
    pub guards: BTreeMap<FileHash, BTreeSet<Loc>>,
}

impl CompilerInfo {
    pub fn new() -> CompilerInfo {
        CompilerInfo::default()
    }

    pub fn from(info: impl IntoIterator<Item = (Loc, CI::IDEAnnotation)>) -> Self {
        let mut result = Self::new();
        result.add_info(info);
        result
    }

    pub fn add_info(&mut self, info: impl IntoIterator<Item = (Loc, CI::IDEAnnotation)>) {
        for (loc, entry) in info {
            match entry {
                CI::IDEAnnotation::MacroCallInfo(info) => {
                    // TODO: should we check this is not also an expanded lambda?
                    // TODO: what if we find two macro calls?
                    if let Some(_old) = self.macro_info.insert(loc, *info) {
                        eprintln!("Repeated macro info");
                    }
                }
                CI::IDEAnnotation::ExpandedLambda => {
                    self.expanded_lambdas.insert(loc);
                }
                CI::IDEAnnotation::DotAutocompleteInfo(info) => {
                    // TODO: what if we find two autocomplete info sets? Intersection may be better
                    // than union, as it's likely in a lambda body.
                    if let Some(_old) = self
                        .dot_autocomplete_info
                        .entry(loc.file_hash())
                        .or_default()
                        .insert(loc, *info)
                    {
                        eprintln!("Repeated autocomplete info");
                    }
                }
                CI::IDEAnnotation::MissingMatchArms(_) => {
                    // TODO: Not much to do with this yet.
                }
                CI::IDEAnnotation::EllipsisMatchEntries(_) => {
                    self.ellipsis_binders.insert(loc);
                }
                CI::IDEAnnotation::PathAutocompleteInfo(info) => {
                    self.path_autocomplete_info.insert(loc, *info);
                }
            }
        }
    }

    pub fn get_macro_info(&mut self, loc: &Loc) -> Option<&CI::MacroCallInfo> {
        self.macro_info.get(loc)
    }

    pub fn is_expanded_lambda(&mut self, loc: &Loc) -> bool {
        self.expanded_lambdas.contains(loc)
    }

    pub fn get_autocomplete_info(
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

    pub fn inside_guard(&self, fhash: FileHash, loc: &Loc, gloc: &Loc) -> bool {
        self.guards
            .get(&fhash)
            .and_then(|guard_locs| guard_locs.get(gloc))
            .is_some()
            && gloc.contains(loc)
    }
}
