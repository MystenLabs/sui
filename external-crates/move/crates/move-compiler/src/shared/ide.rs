// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::Diagnostics, expansion::ast as E, ice, naming::ast as N, parser::ast as P,
    shared::Name, typing::ast as T,
};

use move_ir_types::location::Loc;

use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct IDEInfo {
    pub exp_info: BTreeMap<Loc, ExpEntry>,
}

#[derive(Debug, Clone)]
pub struct ExpEntry {
    /// Location of the recorded info
    pub loc: Loc,
    /// Indicates this location was a macro call site
    pub macro_call_info: Option<Box<MacroCallInfo>>,
    /// Indicates this location is an expansion of a lambda
    pub expanded_lambda: bool,
}

/// For use in the compiler, to ease recording IDE information.
pub enum ExpInfo {
    MacroCallInfo(Box<MacroCallInfo>),
    ExpandedLambda,
}

#[derive(Debug, Clone)]
pub struct MacroCallInfo {
    /// Module where the macro is defined
    pub module: E::ModuleIdent,
    /// Name of the macro function
    pub name: P::FunctionName,
    /// Optional method name if macro invoked as dot-call
    pub method_name: Option<Name>,
    /// Type params at macro's call site
    pub type_arguments: Vec<N::Type>,
    /// By-value args (at this point there should only be one, representing receiver arg)
    pub by_value_args: Vec<T::SequenceItem>,
}

impl IDEInfo {
    pub fn new() -> IDEInfo {
        IDEInfo {
            exp_info: BTreeMap::new(),
        }
    }

    pub fn append(&mut self, diags: &mut Diagnostics, other: &mut IDEInfo) {
        let exp_info = std::mem::take(&mut other.exp_info);
        for (loc, entry) in exp_info {
            if let Some(existing) = self.exp_info.get_mut(&loc) {
                existing.extend_with(diags, entry)
            } else {
                self.exp_info.insert(loc, entry);
            }
        }
    }

    pub fn add_exp_info(&mut self, diags: &mut Diagnostics, loc: Loc, exp_info: ExpInfo) {
        let info = self.exp_info.entry(loc).or_insert(ExpEntry::new(loc));
        match exp_info {
            ExpInfo::MacroCallInfo(minfo) => info.set_macro_call_info(diags, minfo),
            ExpInfo::ExpandedLambda => info.set_expanded_lambda(diags),
        }
    }

    pub fn get_exp_info(&self, loc: &Loc) -> Option<&ExpEntry> {
        self.exp_info.get(loc)
    }
}

impl ExpEntry {
    pub fn new(loc: Loc) -> ExpEntry {
        ExpEntry {
            loc,
            macro_call_info: None,
            expanded_lambda: false,
        }
    }

    pub fn set_expanded_lambda(&mut self, diags: &mut Diagnostics) {
        if self.macro_call_info.is_some() {
            diags.add(ice!((self.loc, "Marked macro call as expanded lambda")))
        }
        self.expanded_lambda = true;
    }

    pub fn set_macro_call_info(&mut self, diags: &mut Diagnostics, info: Box<MacroCallInfo>) {
        if self.expanded_lambda {
            diags.add(ice!((self.loc, "Marked expanded lambda as macro call")))
        }
        if self.macro_call_info.is_some() {
            diags.add(ice!((self.loc, "Re-defined macro call info")))
        }
        self.macro_call_info = Some(info);
    }

    pub fn extend_with(&mut self, diags: &mut Diagnostics, other: ExpEntry) {
        if other.expanded_lambda {
            self.set_expanded_lambda(diags);
        }
        if let Some(minfo) = other.macro_call_info {
            self.set_macro_call_info(diags, minfo);
        }
    }
}
