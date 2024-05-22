// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use move_compiler::shared::ide as CI;
use move_ir_types::location::Loc;

#[derive(Default, Debug, Clone)]
pub struct CompilerInfo {
    pub macro_info: BTreeMap<Loc, CI::MacroCallInfo>,
    pub expanded_lambdas: BTreeSet<Loc>,
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
            }
        }
    }

    pub fn get_macro_info(&mut self, loc: &Loc) -> Option<&CI::MacroCallInfo> {
        self.macro_info.get(loc)
    }

    pub fn is_expanded_lambda(&mut self, loc: &Loc) -> bool {
        self.expanded_lambdas.contains(loc)
    }
}
