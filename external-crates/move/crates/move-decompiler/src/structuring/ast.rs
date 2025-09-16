// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use petgraph::graph::NodeIndex;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

// -----------------------------------------------
// Structuring and Code Types
// -----------------------------------------------

pub type Label = NodeIndex;
pub type Code = (u64, bool);

#[derive(Debug, Clone)]
pub enum Input {
    Condition(Label, Code, Label, Label),
    Variants(Label, Code, Vec<Label>),
    Code(Label, Code, Option<Label>),
}

#[derive(Debug, Clone)]
pub enum Structured {
    Break,
    Continue,
    Block(Code),
    Loop(Box<Structured>),
    Seq(Vec<Structured>),
    While(Code, Box<Structured>),
    IfElse(Code, Box<Structured>, Box<Option<Structured>>),
    Switch(Code, Vec<Structured>),
    Jump(Label),
    JumpIf(Code, Label, Label),
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Input {
    pub fn edges(&self) -> Vec<(NodeIndex, NodeIndex)> {
        match self {
            Input::Condition(lbl, _, then, else_) => vec![(*lbl, *then), (*lbl, *else_)],
            Input::Variants(lbl, _, items) => {
                items.iter().map(|item| (*lbl, *item)).collect::<Vec<_>>()
            }
            Input::Code(lbl, _, Some(to)) => vec![(*lbl, *to)],
            Input::Code(_, _, None) => vec![],
        }
    }

    pub fn label(&self) -> Label {
        match self {
            Input::Condition(lbl, _, _, _)
            | Input::Variants(lbl, _, _)
            | Input::Code(lbl, _, _) => *lbl,
        }
    }
}

impl Structured {
    pub fn to_test_string(&self) -> String {
        format!("{}", self)
    }
}

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------

impl std::fmt::Display for Structured {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn indent(f: &mut std::fmt::Formatter<'_>, level: usize) -> std::fmt::Result {
            for _ in 0..level {
                write!(f, "    ")?;
            }
            Ok(())
        }

        fn fmt_structured(
            s: &Structured,
            f: &mut std::fmt::Formatter<'_>,
            level: usize,
        ) -> std::fmt::Result {
            match s {
                Structured::Block(code) => {
                    indent(f, level)?;
                    writeln!(f, "{{ {:?} }}", code.0)
                }
                Structured::Loop(body) => {
                    indent(f, level)?;
                    writeln!(f, "loop {{")?;
                    fmt_structured(body, f, level + 1)?;
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Structured::While(cond, body) => {
                    indent(f, level)?;
                    if cond.1 {
                        writeln!(f, "while !({:?}) {{", cond.0)?;
                    } else {
                        writeln!(f, "while ({:?}) {{", cond.0)?;
                    }
                    fmt_structured(body, f, level + 1)?;
                    writeln!(f, "}}")
                }
                Structured::IfElse(cond, then_branch, else_branch) => {
                    indent(f, level)?;
                    if cond.1 {
                        writeln!(f, "if !({:?}) {{", cond.0)?;
                    } else {
                        writeln!(f, "if ({:?}) {{", cond.0)?;
                    }
                    fmt_structured(then_branch, f, level + 1)?;
                    indent(f, level)?;
                    if let Some(else_branch) = &**else_branch {
                        writeln!(f, "}} else {{")?;
                        fmt_structured(else_branch, f, level + 1)?;
                        indent(f, level)?;
                    }
                    writeln!(f, "}}")
                }
                Structured::Seq(seq) => {
                    if seq.is_empty() {
                        indent(f, level)?;
                        writeln!(f, "{{ }}")?;
                        return Ok(());
                    }
                    for stmt in seq {
                        fmt_structured(stmt, f, level)?;
                    }
                    Ok(())
                }
                Structured::Switch(expr, arms) => {
                    indent(f, level)?;
                    writeln!(f, "switch ({:?}) {{", expr.0)?;
                    for (ndx, arm) in arms.iter().enumerate() {
                        indent(f, level + 1)?;
                        writeln!(f, "_{ndx} => ")?;
                        fmt_structured(arm, f, level + 2)?;
                    }
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Structured::Break => {
                    indent(f, level)?;
                    writeln!(f, "break;")
                }
                Structured::Continue => {
                    indent(f, level)?;
                    writeln!(f, "continue;")
                }
                Structured::Jump(node_index) => {
                    indent(f, level)?;
                    writeln!(f, "jump {:?};", node_index)
                }
                Structured::JumpIf(_, node_index, node_index1) => {
                    indent(f, level)?;
                    writeln!(f, "jump_if ({:?}, {:?});", node_index, node_index1)
                }
            }
        }

        fmt_structured(self, f, 0)
    }
}
