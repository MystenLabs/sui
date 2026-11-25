// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Renders every span in a module's HLIR as a `line:column` gutter beside an indented node tree.
//!
//! Lowering invents nodes that have no syntax of their own, and the span each one is given is
//! otherwise invisible until it reaches a debugger or a coverage report. Dumping the whole body
//! keeps a generated node's span reviewable next to the nodes it was built from.

use move_compiler::{hlir::ast as H, parser::ast::TargetKind, shared::files::MappedFiles};
use move_ir_types::location::Loc;

/// Longest source snippet shown before it is cut short.
const SNIPPET_WIDTH: usize = 48;
/// Width of the `line:column` gutter.
const LOC_WIDTH: usize = 11;
/// Width of the indented node name, so snippets line up across depths.
const TREE_WIDTH: usize = 30;

pub fn dump(prog: &H::Program, files: &MappedFiles) -> String {
    let mut d = Dumper {
        files,
        out: Vec::new(),
    };
    for (mident, mdef) in prog.modules.key_cloned_iter() {
        // Dependencies are compiled alongside the fixture but are not what the test is about.
        if !matches!(
            mdef.target_kind,
            TargetKind::Source {
                is_root_package: true
            }
        ) {
            continue;
        }
        d.out.push(format!("module {mident}"));
        for (name, cdef) in mdef.constants.key_cloned_iter() {
            d.out.push(format!("  const {name}"));
            d.block(&cdef.value.1, 2);
        }
        for (name, fdef) in mdef.functions.key_cloned_iter() {
            d.out.push(format!("  fun {name}"));
            if let H::FunctionBody_::Defined { body, .. } = &fdef.body.value {
                d.block(body, 2);
            }
        }
    }
    d.out.join("\n")
}

struct Dumper<'a> {
    files: &'a MappedFiles,
    out: Vec<String>,
}

impl Dumper<'_> {
    fn row(&mut self, depth: usize, kind: &str, loc: Loc) {
        let position = match self.files.position_opt(&loc) {
            Some(position) => {
                let (start, end) = (position.start, position.end);
                // A span that stays on one line does not repeat the line number.
                if start.user_line() == end.user_line() {
                    format!(
                        "{}:{}-{}",
                        start.user_line(),
                        start.user_column(),
                        end.user_column()
                    )
                } else {
                    format!(
                        "{}:{}-{}:{}",
                        start.user_line(),
                        start.user_column(),
                        end.user_line(),
                        end.user_column()
                    )
                }
            }
            None => "?".to_string(),
        };
        // Collapsed to one line so a span covering many lines still reads as source.
        let snippet = match self.files.source_of_loc_opt(&loc) {
            Some(source) => {
                let text = source.split_whitespace().collect::<Vec<_>>().join(" ");
                match text.char_indices().nth(SNIPPET_WIDTH) {
                    Some((i, _)) => format!("{}…", &text[..i]),
                    None => text,
                }
            }
            None => "<unknown file>".to_string(),
        };
        let tree = format!("{:indent$}{kind}", "", indent = depth * 2);
        self.out.push(format!(
            "{position:<LOC_WIDTH$} {tree:<TREE_WIDTH$} {snippet}"
        ));
    }

    fn block(&mut self, block: &H::Block, depth: usize) {
        for stmt in block {
            self.stmt(stmt, depth)
        }
    }

    fn stmt(&mut self, stmt: &H::Statement, depth: usize) {
        use H::Statement_ as S;
        match &stmt.value {
            S::Command(cmd) => self.cmd(cmd, depth),
            S::IfElse {
                cond,
                if_block,
                else_block,
            } => {
                self.row(depth, "IfElse", stmt.loc);
                self.exp(cond, depth + 1);
                self.block(if_block, depth + 1);
                self.block(else_block, depth + 1);
            }
            S::VariantMatch { subject, arms, .. } => {
                self.row(depth, "VariantMatch", stmt.loc);
                self.exp(subject, depth + 1);
                for (_, block) in arms {
                    self.block(block, depth + 1)
                }
            }
            S::While {
                cond: (cond_block, cond),
                block,
                ..
            } => {
                self.row(depth, "While", stmt.loc);
                self.block(cond_block, depth + 1);
                self.exp(cond, depth + 1);
                self.block(block, depth + 1);
            }
            S::Loop { block, .. } => {
                self.row(depth, "Loop", stmt.loc);
                self.block(block, depth + 1);
            }
            S::NamedBlock { block, .. } => {
                self.row(depth, "NamedBlock", stmt.loc);
                self.block(block, depth + 1);
            }
        }
    }

    fn cmd(&mut self, cmd: &H::Command, depth: usize) {
        use H::Command_ as C;
        let kind = match &cmd.value {
            C::Assign(..) => "Assign",
            C::Mutate(..) => "Mutate",
            C::Abort(..) => "Abort",
            C::Return { .. } => "Return",
            C::Break(_) => "Break",
            C::Continue(_) => "Continue",
            C::IgnoreAndPop { .. } => "IgnoreAndPop",
            C::Jump { .. } => "Jump",
            C::JumpIf { .. } => "JumpIf",
            C::VariantSwitch { .. } => "VariantSwitch",
        };
        self.row(depth, kind, cmd.loc);
        match &cmd.value {
            C::Assign(_, _, e)
            | C::Abort(_, e)
            | C::Return { exp: e, .. }
            | C::IgnoreAndPop { exp: e, .. }
            | C::JumpIf { cond: e, .. }
            | C::VariantSwitch { subject: e, .. } => self.exp(e, depth + 1),
            C::Mutate(lhs, rhs) => {
                self.exp(lhs, depth + 1);
                self.exp(rhs, depth + 1)
            }
            C::Break(_) | C::Continue(_) | C::Jump { .. } => (),
        }
    }

    fn exp(&mut self, exp: &H::Exp, depth: usize) {
        use H::UnannotatedExp_ as E;
        let kind = match &exp.exp.value {
            E::Unit { .. } => "Unit",
            E::Value(_) => "Value",
            E::Move { .. } => "Move",
            E::Copy { .. } => "Copy",
            E::Constant(..) => "Constant",
            E::ErrorConstant { .. } => "ErrorConstant",
            E::ModuleCall(_) => "ModuleCall",
            E::Freeze(_) => "Freeze",
            E::Vector(..) => "Vector",
            E::Dereference(_) => "Dereference",
            E::UnaryExp(..) => "UnaryExp",
            E::BinopExp(..) => "BinopExp",
            E::Pack(..) => "Pack",
            E::PackVariant(..) => "PackVariant",
            E::Multiple(_) => "Multiple",
            E::Borrow(..) => "Borrow",
            E::BorrowLocal(..) => "BorrowLocal",
            E::Cast(..) => "Cast",
            E::Unreachable => "Unreachable",
            E::UnresolvedError => "UnresolvedError",
        };
        self.row(depth, kind, exp.exp.loc);
        match &exp.exp.value {
            E::Freeze(inner)
            | E::Dereference(inner)
            | E::UnaryExp(_, inner)
            | E::Cast(inner, _)
            | E::Borrow(_, inner, _, _) => self.exp(inner, depth + 1),
            E::BinopExp(lhs, _, rhs) => {
                self.exp(lhs, depth + 1);
                self.exp(rhs, depth + 1)
            }
            E::ModuleCall(call) => {
                for arg in &call.arguments {
                    self.exp(arg, depth + 1)
                }
            }
            E::Vector(_, _, _, exps) | E::Multiple(exps) => {
                for e in exps {
                    self.exp(e, depth + 1)
                }
            }
            E::Pack(_, _, fields) => {
                for (_, _, e) in fields {
                    self.exp(e, depth + 1)
                }
            }
            E::PackVariant(_, _, _, fields) => {
                for (_, _, e) in fields {
                    self.exp(e, depth + 1)
                }
            }
            E::Unit { .. }
            | E::Value(_)
            | E::Move { .. }
            | E::Copy { .. }
            | E::Constant(..)
            | E::ErrorConstant { .. }
            | E::BorrowLocal(..)
            | E::Unreachable
            | E::UnresolvedError => (),
        }
    }
}
