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
    /// Condition whose arm targets the condition node itself (back-edge to the loop head).
    DegenerateJumpIf,
    /// Arm target is the loop-head's chosen successor; `insert_breaks` rewrites this to
    /// `Break(loop_head)`.
    LoopBreak,
    /// Arm target sits outside `start`'s dominator subtree, or is the IfElse/Switch's join
    /// point. Either way, the owned-children hoist may place the target as a sibling and
    /// elide this Jump; if it survives, `insert_breaks` reclassifies for the enclosing
    /// loop, or `generate_output` lowers to `Unstructured`.
    ArmOutsideSubtree,
    /// Jump emitted by `structure_code_node` when the Code block's `next` isn't its
    /// dom-tree child — the join is owned by an enclosing scope. Without this explicit
    /// Jump the branch would live only in the bytecode terminator, invisible to elision.
    CodeBranch,
    /// JumpIf emitted at a latch node by `structure_latch_node`.
    LatchTest,
    /// Jump emitted at a latch node's Code-input by `structure_latch_node`.
    LatchCode,
    /// Self-edge Jump emitted by `structure_code_node` for a code block whose `next` is
    /// itself. Suspected unreachable in practice.
    SelfLoop,
    /// Escape Jump synthesized in `insert_breaks` when a JumpIf has one Latch arm.
    EscapeJumpIf,
    /// Jump emitted by the reaching-condition structurer at a region-exit edge — either the
    /// loop-body back edge (target == loop_head, rewritten to `Continue` by `insert_breaks`)
    /// or a break-target edge (target outside the loop, rewritten to `Break`).
    ReachingExit,
}

impl GotoSource {
    pub fn as_tag(&self) -> &'static str {
        match self {
            GotoSource::DegenerateJumpIf => "DJI",
            GotoSource::LoopBreak => "LB",
            GotoSource::ArmOutsideSubtree => "AOS",
            GotoSource::CodeBranch => "CB",
            GotoSource::LatchTest => "LT",
            GotoSource::LatchCode => "LC",
            GotoSource::SelfLoop => "SL",
            GotoSource::EscapeJumpIf => "EJI",
            GotoSource::ReachingExit => "RE",
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
    /// An `if`/`else` whose guard is a `Formula` over branch-condition atoms (block ids).
    /// `Formula::Atom(code)` is the degenerate single-block case (the dom-tree structurer's
    /// product); compound `And`/`Or`/`Not` formulas come from the reaching-condition acyclic
    /// structurer recovering a guarded forward skip without a goto. Lowered to `Exp::IfElse`
    /// by substituting each atom with its block's condition expression and threading
    /// `&&`/`||`/`!` through.
    CondIf(
        crate::structuring::predicates::Formula,
        Box<Structured>,
        Box<Option<Structured>>,
    ),
    Switch(
        Code,
        /* enum */ (ModuleId<Symbol>, Symbol),
        /* variant x rhs */ Vec<(Symbol, Structured)>,
    ),
    /// Goto. `GotoSource` records which structurer path created it for instrumentation.
    Jump(GotoSource, Label),
    /// Two-way goto. Same instrumentation as `Jump`.
    JumpIf(GotoSource, Code, Label, Label),
    /// Synthetic declaration of a dispatch local emitted by `structure_loop` for multi-succ
    /// loops: `let <name>: u32;`. Translated to `Exp::Declare`.
    Let(String),
    /// Synthetic assignment of an integer tag to a dispatch local: `<name> = <value>;`.
    /// Emitted at each exit site inside a multi-succ loop body to mark which arm to
    /// dispatch. Translated to `Exp::Assign(name, Constant(value))`.
    Assign(String, crate::ast::DispatchTag),
    /// Synthetic integer-literal match emitted after a multi-succ loop:
    /// `match (<name>) { 0 => ..., 1 => ..., }`. Translated to `Exp::MatchLit`.
    SelectorMatch(String, Vec<(crate::ast::DispatchTag, Structured)>),
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

    /// `Block(codes[0])` if `codes.len() == 1`; otherwise a `Seq` of `Block`s.
    pub fn blocks_seq(codes: &[Code]) -> Structured {
        if codes.len() == 1 {
            Structured::Block(codes[0])
        } else {
            Structured::Seq(codes.iter().map(|c| Structured::Block(*c)).collect())
        }
    }

    /// `Jump(GotoSource::ReachingExit, target)`.
    pub fn exit_jump(target: NodeIndex) -> Structured {
        Structured::Jump(GotoSource::ReachingExit, target)
    }

    /// Concatenate two structured forms, flattening any `Seq`s into the result so we don't
    /// accumulate `Seq(Seq(...))` nesting.
    pub fn seq(head: Structured, tail: Structured) -> Structured {
        let mut items = Vec::new();
        let mut push = |s: Structured| match s {
            Structured::Seq(v) => items.extend(v),
            other => items.push(other),
        };
        push(head);
        push(tail);
        Self::seq_or_singleton(items)
    }

    /// Empty input -> `Seq([])`; single-item input -> that item bare; otherwise -> `Seq`.
    /// Avoids the `Seq([x])` shape that downstream refinements would just unwrap anyway.
    pub fn seq_or_singleton(mut items: Vec<Structured>) -> Structured {
        match items.len() {
            0 => Structured::Seq(vec![]),
            1 => items.pop().unwrap(),
            _ => Structured::Seq(items),
        }
    }

    /// `None` if `self` is an empty `Seq`; otherwise `Some(self)`. Useful for `IfElse` else
    /// arms that should be elided rather than rendered as `else { }`.
    pub fn non_empty(self) -> Option<Structured> {
        match &self {
            Structured::Seq(v) if v.is_empty() => None,
            _ => Some(self),
        }
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
                Structured::CondIf(cond, then_branch, else_branch) => {
                    indent(f, level)?;
                    // Single-atom guard renders as bare block id; compound formulas render
                    // inside `<...>` so debug output stays scannable.
                    match cond.as_cond_atom() {
                        Some(n) => writeln!(f, "if ({}) {{", n.index())?,
                        None => writeln!(f, "if <{cond}> {{")?,
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
                Structured::Let(name) => {
                    indent(f, level)?;
                    writeln!(f, "let {name}: u32;")
                }
                Structured::Assign(name, value) => {
                    indent(f, level)?;
                    writeln!(f, "{name} = {value};")
                }
                Structured::SelectorMatch(name, arms) => {
                    indent(f, level)?;
                    writeln!(f, "match ({name}) {{")?;
                    for (lit, body) in arms {
                        indent(f, level + 1)?;
                        writeln!(f, "{lit} => {{")?;
                        fmt_structured(body, f, level + 2)?;
                        indent(f, level + 1)?;
                        writeln!(f, "}},")?;
                    }
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
            }
        }

        fmt_structured(self, f, 0)
    }
}
