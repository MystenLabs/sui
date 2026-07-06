// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::structuring::predicates::{self, Formula};
use move_binary_format::normalized::ModuleId;
use move_symbol_pool::Symbol;
use petgraph::graph::NodeIndex;
use std::collections::HashSet;

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
    /// Already-structured abstract node (NMG §IV-C collapse). The structured form lives
    /// in `structured_blocks[label]`; CFG-wise this node has `succs` as its out-edges.
    /// Installed by `structure_loop` after a loop body is wrapped, so outer scopes treat
    /// the loop as a single opaque block.
    Reduced(Label, Vec<Label>),
}

/// Provenance for a surviving `Jump`. Each variant names the structurer path that
/// created the goto; the tag rides through `insert_breaks` and is printed on stderr when a
/// Jump is lowered to `Unstructured(Goto)` in `generate_output`, letting the corpus driver
/// attribute residual gotos by source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GotoSource {
    /// Jump emitted by the reaching-condition structurer at a region-exit edge - either the
    /// loop-body back edge (target == loop_head, rewritten to `Continue` by `insert_breaks`)
    /// or a break-target edge (target outside the loop, rewritten to `Break`).
    ReachingExit,
}

impl GotoSource {
    pub fn as_tag(&self) -> &'static str {
        match self {
            GotoSource::ReachingExit => "RE",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Structured {
    /// `break 'label;` - targets the labeled enclosing Loop. Structuring always knows which
    /// loop a break targets (the loop being processed), so this is unconditional `Label`. The
    /// `Option`al/unlabeled form lives in `crate::ast::Exp` after `strip_loop_labels` runs.
    Break(Label),
    /// `continue 'label;` - see `Break`.
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
    /// Synthetic declaration of a dispatch local emitted by `structure_loop` for multi-succ
    /// loops: `let <name>: u32;`. Translated to `Exp::Declare`.
    Let(String),
    /// Synthetic assignment of an integer tag to a dispatch local: `<name> = <value>;`.
    /// Emitted at each exit site inside a multi-succ loop body to mark which arm to
    /// dispatch. Translated to `Exp::Assign(name, Constant(value))`.
    AssignTag(String, crate::ast::DispatchTag),
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
            Input::Reduced(lbl, succs) => succs.iter().map(|s| (*lbl, *s)).collect(),
        }
    }

    pub fn label(&self) -> Label {
        match self {
            Input::Condition(lbl, _, _, _)
            | Input::Variants(lbl, _, _, _)
            | Input::Code(lbl, _, _)
            | Input::Reduced(lbl, _) => *lbl,
        }
    }
}

impl Structured {
    pub fn to_test_string(&self) -> String {
        format!("{}", self)
    }

    /// `Jump(GotoSource::ReachingExit, target)`.
    pub fn exit_jump(target: NodeIndex) -> Structured {
        Structured::Jump(GotoSource::ReachingExit, target)
    }

    /// Every `Jump(_, target)` reachable in `self`, as sorted-deduped target labels.
    /// A non-empty result means the structurer left raw control-flow residue that survived
    /// every refinement pass - the output will emit `unstructured { goto 'label_N }` for
    /// each one. Used by the test harness + pretty-printer to surface residue.
    pub fn collect_jump_targets(&self) -> Vec<u64> {
        fn walk(s: &Structured, out: &mut Vec<u64>) {
            match s {
                Structured::Jump(_, target) => out.push(target.index() as u64),
                Structured::Seq(items) => items.iter().for_each(|i| walk(i, out)),
                Structured::CondIf(_, conseq, alt) => {
                    walk(conseq, out);
                    if let Some(a) = alt.as_ref().as_ref() {
                        walk(a, out);
                    }
                }
                Structured::Loop(_, body) => walk(body, out),
                Structured::Switch(_, _, arms) => arms.iter().for_each(|(_, b)| walk(b, out)),
                Structured::SelectorMatch(_, arms) => {
                    arms.iter().for_each(|(_, b)| walk(b, out));
                }
                Structured::Block(_)
                | Structured::Break(_)
                | Structured::Continue(_)
                | Structured::Let(_)
                | Structured::AssignTag(_, _) => {}
            }
        }
        let mut out = Vec::new();
        walk(self, &mut out);
        out.sort_unstable();
        out.dedup();
        out
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

    /// `Seq[a, b]` flattened: if either side is already a `Seq`, its items splice in
    /// rather than nesting. The structurer's refinement passes leave both sides clean,
    /// so this never recurs deeper than one level.
    pub fn splice_seq(a: Structured, b: Structured) -> Structured {
        let mut out: Vec<Structured> = Vec::new();
        match a {
            Structured::Seq(items) => out.extend(items),
            other => out.push(other),
        }
        match b {
            Structured::Seq(items) => out.extend(items),
            other => out.push(other),
        }
        Structured::Seq(out)
    }

    /// Build a `Seq` from a list of `(guard, body)` items. `True`-guarded items emit
    /// bare; non-`True` guards wrap in `CondIf(g, body, None)`.
    ///
    /// We emit the guard in the form the smart constructors produced - NNF, sorted,
    /// deduped, with classical absorption (`A || (A && X) -> A`), complementary collapse,
    /// and distributive factoring (`(A && X) || (A && Y) -> A && (X || Y)`) already
    /// applied. We use QM (`Formula::simplify`) only to detect tautologies and
    /// contradictions - if QM returns `True` we elide the wrapper, if `False` we drop
    /// the item, and otherwise we discard QM's output and keep the original guard. The
    /// reason is QM's absorption-with-complement rule `A || (!A && B) <-> A || B`: sound
    /// as a boolean function but destroys the structural conditioning the smart
    /// constructors maintain - if `B`'s atoms are condition-block locals only assigned
    /// along the `!A` path, the QM-collapsed output reads them on the `A` path where
    /// they're definitely unassigned, producing invalid Move source. Restricting QM to
    /// the True/False outcomes lets us still elide tautological wrappers (multi-atom
    /// reach-condition disjunctions that cover the truth space) without paying that
    /// cost on guards that should keep their structure.
    pub fn from_guarded_items(items: Vec<(Formula, Structured)>) -> Structured {
        let mut out: Vec<Structured> = Vec::with_capacity(items.len());
        for (guard, body) in items {
            match guard.classify() {
                Some(true) => out.push(body),
                Some(false) => {}
                None => out.push(Structured::CondIf(guard, Box::new(body), Box::new(None))),
            }
        }
        Structured::Seq(out)
    }

    /// True iff every path through `self` leaves the surrounding sibling sequence:
    /// `Break`/`Continue`/`Jump`/`JumpIf`; a `Block(code)` whose `code` is a CFG sink
    /// (abort/return); a `Seq` whose last item terminates; or a `CondIf` whose both
    /// arms terminate. `Loop`/`Switch`/`SelectorMatch` are treated as non-terminating
    /// since their iteration / branch shapes aren't analyzed here.
    pub fn always_terminates(&self, sink_codes: &HashSet<u64>) -> bool {
        match self {
            Structured::Break(_) | Structured::Continue(_) | Structured::Jump(..) => true,
            Structured::Block(code) => sink_codes.contains(code),
            Structured::Seq(items) => items
                .last()
                .is_some_and(|x| x.always_terminates(sink_codes)),
            Structured::CondIf(_, then, alt) => {
                then.always_terminates(sink_codes)
                    && alt
                        .as_ref()
                        .as_ref()
                        .is_some_and(|a| a.always_terminates(sink_codes))
            }
            _ => false,
        }
    }

    /// Walk `self` collecting assumptions implied by terminators. `guard_stack` is the
    /// conjunction of enclosing `CondIf` conditions. In a `Seq`, an early-exit `CondIf`
    /// lets subsequent siblings assume the complement; assumptions get lifted to the
    /// outer scope as `guard_stack -> local`.
    pub fn terminator_assumptions(
        &self,
        guard_stack: &[Formula],
        sink_codes: &HashSet<u64>,
    ) -> Vec<Formula> {
        let mut out = Vec::new();
        self.collect_terminator_assumptions(guard_stack, sink_codes, &mut out);
        out
    }

    fn collect_terminator_assumptions(
        &self,
        guard_stack: &[Formula],
        sink_codes: &HashSet<u64>,
        out: &mut Vec<Formula>,
    ) {
        fn lift(local: Formula, gs: &[Formula]) -> Formula {
            if gs.is_empty() {
                return local;
            }
            let guard_conj = predicates::and(gs.to_vec());
            predicates::or(vec![predicates::not(guard_conj), local])
        }
        match self {
            Structured::Seq(items) => {
                let mut local: Vec<Formula> = Vec::new();
                for item in items {
                    let mut local_stack: Vec<Formula> = guard_stack.to_vec();
                    local_stack.extend(local.iter().cloned());
                    item.collect_terminator_assumptions(&local_stack, sink_codes, out);
                    // Three early-exit shapes inside this Seq:
                    //   - `CondIf(c, term, None)`           -> assume !c for siblings.
                    //   - `CondIf(c, term, Some(non_term))` -> assume !c (took non-term).
                    //   - `CondIf(c, non_term, Some(term))` -> assume  c (took non-term).
                    if let Structured::CondIf(g, body, alt) = item {
                        let then_term = body.always_terminates(sink_codes);
                        let alt_term = alt
                            .as_ref()
                            .as_ref()
                            .is_some_and(|a| a.always_terminates(sink_codes));
                        match (then_term, alt_term, alt.as_ref().as_ref().is_some()) {
                            (true, _, false) => local.push(predicates::not(g.clone())),
                            (true, false, true) => local.push(predicates::not(g.clone())),
                            (false, true, true) => local.push(g.clone()),
                            _ => {}
                        }
                    }
                }
                for l in local {
                    out.push(lift(l, guard_stack));
                }
            }
            Structured::CondIf(g, then, alt) => {
                let mut then_stack: Vec<Formula> = guard_stack.to_vec();
                then_stack.push(g.clone());
                then.collect_terminator_assumptions(&then_stack, sink_codes, out);
                if let Some(a) = alt.as_ref().as_ref() {
                    let mut else_stack: Vec<Formula> = guard_stack.to_vec();
                    else_stack.push(predicates::not(g.clone()));
                    a.collect_terminator_assumptions(&else_stack, sink_codes, out);
                }
            }
            // Loop bodies may run zero or many times - we can't carry assumptions through.
            Structured::Loop(..) => {}
            _ => {}
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
                Structured::Let(name) => {
                    indent(f, level)?;
                    writeln!(f, "let {name}: u32;")
                }
                Structured::AssignTag(name, value) => {
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
