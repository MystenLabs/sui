// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::normalized::ModuleId;

use move_symbol_pool::Symbol;
use petgraph::graph::NodeIndex;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

// -----------------------------------------------
// Structuring and Code Types
// -----------------------------------------------

pub type Label = NodeIndex;
// The bool indicates whether the condition is negated
pub type Code = u64;

#[derive(Debug, Clone)]
pub enum Input {
    Condition(Label, Code, Label, Label),
    Variants(
        Label,
        Code,
        /* enum */ (ModuleId<Symbol>, Symbol),
        /* variant x label */ Vec<(Symbol, Label)>,
    ),
    Code(Label, Code, Option<Label>),
}

/// Provenance for a surviving `Jump`/`JumpIf`. Each variant names the structurer path that
/// created the goto; the tag rides through `insert_breaks` and is printed on stderr when a
/// Jump is lowered to `Unstructured(Goto)` in `generate_output`, letting the corpus driver
/// attribute residual gotos by source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GotoSource {
    /// Then-arm of a Condition targets the post-dominator.
    ConseqEqPostDom,
    /// Else-arm of a Condition targets the post-dominator.
    AltEqPostDom,
    /// Condition whose arm targets the condition node itself (back-edge to the loop head).
    DegenerateJumpIf,
    /// Arm target sits outside start's dominator subtree (typically a Continue/Break to an
    /// enclosing loop, rewritten later by that loop's `insert_breaks`).
    ArmOutsideSubtree,
    /// JumpIf emitted at a latch node by `structure_latch_node`.
    LatchTest,
    /// Jump emitted at a latch node's Code-input by `structure_latch_node`.
    LatchCode,
    /// Self-edge Jump emitted by `structure_code_node` for a code block whose `next` is
    /// itself. Suspected unreachable in practice.
    SelfLoop,
    /// Escape Jump synthesized in `insert_breaks` when a JumpIf has one Latch arm.
    EscapeJumpIf,
}

impl GotoSource {
    pub fn as_tag(&self) -> &'static str {
        match self {
            GotoSource::ConseqEqPostDom => "CPD",
            GotoSource::AltEqPostDom => "APD",
            GotoSource::DegenerateJumpIf => "DJI",
            GotoSource::ArmOutsideSubtree => "AOS",
            GotoSource::LatchTest => "LT",
            GotoSource::LatchCode => "LC",
            GotoSource::SelfLoop => "SL",
            GotoSource::EscapeJumpIf => "EJI",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Structured {
    /// `break 'label;` — targets the labeled enclosing Loop. Structuring always knows which
    /// loop a break targets (the loop being processed), so this is unconditional `Label`. The
    /// `Option`al/unlabeled form lives in `crate::ast::Exp` after `strip_loop_labels` runs.
    Break(Label),
    /// `continue 'label;` — see `Break`.
    Continue(Label),
    Block(Code),
    /// `'label: loop { ... }`. The label is the loop_head NodeIndex; it disambiguates
    /// labeled `Break`/`Continue` from inner loops that target this one.
    Loop(Label, Box<Structured>),
    Seq(Vec<Structured>),
    IfElse(Code, Box<Structured>, Box<Option<Structured>>),
    Switch(
        Code,
        /* enum */ (ModuleId<Symbol>, Symbol),
        /* variant x rhs */ Vec<(Symbol, Structured)>,
    ),
    /// Goto. `GotoSource` records which structurer path created it for instrumentation.
    Jump(GotoSource, Label),
    /// Two-way goto. Same instrumentation as `Jump`.
    JumpIf(GotoSource, Code, Label, Label),
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Input {
    pub fn edges(&self) -> Vec<(NodeIndex, NodeIndex)> {
        match self {
            Input::Condition(lbl, _, then, else_) => vec![(*lbl, *then), (*lbl, *else_)],
            Input::Variants(lbl, _, _, items) => items
                .iter()
                .map(|(_, item)| (*lbl, *item))
                .collect::<Vec<_>>(),
            Input::Code(lbl, _, Some(to)) => vec![(*lbl, *to)],
            Input::Code(_, _, None) => vec![],
        }
    }

    pub fn label(&self) -> Label {
        match self {
            Input::Condition(lbl, _, _, _)
            | Input::Variants(lbl, _, _, _)
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
                    writeln!(f, "{{ {:?} }}", code)
                }
                Structured::Loop(label, body) => {
                    indent(f, level)?;
                    writeln!(f, "'loop_{}: loop {{", label.index())?;
                    fmt_structured(body, f, level + 1)?;
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Structured::IfElse(cond, then_branch, else_branch) => {
                    indent(f, level)?;
                    writeln!(f, "if ({:?}) {{", cond)?;
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
                Structured::Switch(expr, _, arms) => {
                    indent(f, level)?;
                    writeln!(f, "switch ({:?}) {{", expr)?;
                    for (ndx, (_variant, arm)) in arms.iter().enumerate() {
                        indent(f, level + 1)?;
                        writeln!(f, "_{ndx} => ")?;
                        fmt_structured(arm, f, level + 2)?;
                    }
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Structured::Break(label) => {
                    indent(f, level)?;
                    writeln!(f, "break 'loop_{};", label.index())
                }
                Structured::Continue(label) => {
                    indent(f, level)?;
                    writeln!(f, "continue 'loop_{};", label.index())
                }
                Structured::Jump(src, node_index) => {
                    indent(f, level)?;
                    writeln!(f, "jump<{}> {:?};", src.as_tag(), node_index)
                }
                Structured::JumpIf(src, _, node_index, node_index1) => {
                    indent(f, level)?;
                    writeln!(
                        f,
                        "jump_if<{}> ({:?}, {:?});",
                        src.as_tag(),
                        node_index,
                        node_index1
                    )
                }
            }
        }

        fmt_structured(self, f, 0)
    }
}
