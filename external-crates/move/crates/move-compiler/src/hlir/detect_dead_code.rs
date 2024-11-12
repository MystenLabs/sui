// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::{warning_filters::WarningFilters, Diagnostic, DiagnosticReporter, Diagnostics},
    expansion::ast::ModuleIdent,
    ice,
    naming::ast::{self as N, BlockLabel},
    parser::ast::BinOp_,
    shared::{unique_map::UniqueMap, *},
    typing::ast as T,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use std::collections::{BTreeSet, VecDeque};

//**************************************************************************************************
// Description
//**************************************************************************************************
// This analysis considers the input for potentially dead code due to control flow. It tracks
// control flow in a somewhat fine-grained way, and when it finds a position that diverges it
// reports that as an error.
//
// For simplicity, it aims to satify the following requirements:
//
//     1. For each block, if we discover a divergent instruction either at the top level or
//        embedded in a value position (e.g., the RHS of a let, or tail-value position), we report
//        that user to the error as possible dead code, under the following guidelines:.
//        a) If the divergent code is nested within a value, we report it as a value error.
//        b) If the divergent code is in a statement position and the block has a trailing unit as
//           its last expression, report it as a trailing semicolon error.
//        c) If the divergent code is in any other statement position, report it as such.
//        d) If both arms of an if diverge in the same way in value or tail position, report the
//           entire if together.
//
//     2. We only report the first such error we find, as described above, per-block. For example,
//        this will only yield one error, pointing at the first line:
//            {
//                1 + loop {};
//                1 + loop {};
//            }
//
//     3. If we discover a malformed sub-expression, we do not return a further error.
//        For example, we would report a trailing semicolon error for this:
//            {
//                if (true) { return 0 } else { return 1 };
//            }
//        However, we will not for this `if`, only its inner arms, as they are malformed:
//            {
//                if (true) { return 0; } else { return 1; };
//            }
//        This is because the former case has two well-formed sub-blocks, but the latter case is
//        already going to raise warnings for each of the sub-block cases.
//
//  The implementation proceeds as a context-based walk, considering `tail` (or `return`) position,
//  `value` position, and `statement` position. Errors are reported differently depending upon
//  where they are found.

//**************************************************************************************************
// Context
//**************************************************************************************************

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ControlFlowEntry {
    AbortCalled,
    InfiniteLoop,
    GiveCalled(BlockLabel),     // tracks the name
    ContinueCalled(BlockLabel), // tracks the name
    ReturnCalled,
}

type ControlFlowSet = BTreeSet<ControlFlowEntry>;

#[derive(Debug)]
enum ControlFlow {
    None,
    Possible,
    Divergent {
        loc: Loc,
        set: ControlFlowSet,
        reported: bool,
    },
}

impl ControlFlow {
    fn is_none(&self) -> bool {
        match self {
            ControlFlow::None => true,
            ControlFlow::Divergent { .. } | ControlFlow::Possible => false,
        }
    }

    fn is_some(&self) -> bool {
        match self {
            ControlFlow::None => false,
            ControlFlow::Divergent { .. } | ControlFlow::Possible => true,
        }
    }

    // Combinse control flows around branch arms:
    // - If both are None, None
    // - If both are Divergent, Divergent
    // - If either is Possible, divergence is Possible
    fn combine_arms(self, loc: Loc, other: Self) -> Self {
        use ControlFlow as CF;
        match self {
            CF::None => match other {
                CF::None => CF::None,
                CF::Possible => CF::Possible,
                CF::Divergent { .. } => CF::Possible,
            },
            CF::Possible => CF::Possible,
            CF::Divergent {
                loc: _,
                set: mut left,
                reported: reported_left,
            } => match other {
                CF::None => CF::Possible,
                CF::Possible => CF::Possible,
                CF::Divergent {
                    loc: _,
                    set: mut right,
                    reported: reported_right,
                } => {
                    left.append(&mut right);
                    CF::Divergent {
                        loc,
                        set: left,
                        reported: reported_left || reported_right,
                    }
                }
            },
        }
    }

    // Combinse control flows around sequence entries (assuming `self` comes before `next`):
    // - If `self` diverges, divergent.
    // - If `self` is None, whatever `next` does.
    // - If `self` is Possible, consider `next`:
    //   - `None` means divergence is Possible
    //   - `Divergent` means divergence is Divergence, combining sets. This code:
    //         ...
    //         if (cond) break 'a;
    //         break 'b;
    //         ...
    //      Produces Divergent({a, b})
    //      divergence,
    //   - `Possible` means divergence is Possible, combining
    fn combine_seq(self, next: Self) -> Self {
        use ControlFlow as CF;
        match self {
            CF::Divergent { loc, set, reported } => CF::Divergent { loc, set, reported },
            CF::None => next,
            CF::Possible => match next {
                CF::None => CF::Possible,
                CF::Divergent { .. } => CF::Possible,
                CF::Possible => CF::Possible,
            },
        }
    }

    fn remove_label(mut self, label: &BlockLabel) -> Self {
        use ControlFlow as CF;
        match &mut self {
            CF::None | CF::Possible => (),
            CF::Divergent {
                set,
                reported: _,
                loc: _,
            } => {
                set.remove(&ControlFlowEntry::GiveCalled(*label));
                set.remove(&ControlFlowEntry::ContinueCalled(*label));
                if set.is_empty() {
                    return CF::None;
                }
            }
        };
        self
    }

    fn is_divergent(&self) -> bool {
        match self {
            ControlFlow::Divergent { .. } => true,
            ControlFlow::None | ControlFlow::Possible => false,
        }
    }
}

struct Context<'env> {
    #[allow(unused)]
    env: &'env CompilationEnv,
    reporter: DiagnosticReporter<'env>,
    // loops: Vec<BlockLabel>,
}

impl<'env> Context<'env> {
    pub fn new(env: &'env CompilationEnv) -> Self {
        // let loops = vec![];
        // Context { env , loops }
        let reporter = env.diagnostic_reporter_at_top_level();
        Context { env, reporter }
    }

    pub fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    #[allow(unused)]
    pub fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    pub fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }

    fn maybe_report_value_error(&mut self, error: &mut ControlFlow) -> bool {
        use ControlFlow as CF;
        match error {
            CF::Divergent {
                loc,
                set: _,
                reported,
            } if !*reported => {
                *reported = true;
                self.add_diag(diag!(UnusedItem::DeadCode, (*loc, VALUE_UNREACHABLE_MSG)));
                true
            }
            CF::Divergent { .. } | CF::None | CF::Possible => false,
        }
    }

    fn maybe_report_tail_error(&mut self, error: &mut ControlFlow) -> bool {
        use ControlFlow as CF;
        match error {
            CF::Divergent {
                loc,
                set: _,
                reported,
            } if !*reported => {
                *reported = true;
                self.add_diag(diag!(UnusedItem::DeadCode, (*loc, DIVERGENT_MSG)));
                true
            }
            CF::Divergent { .. } | CF::None | CF::Possible => false,
        }
    }

    fn maybe_report_statement_error(
        &mut self,
        error: &mut ControlFlow,
        next_stmt: Option<&Loc>,
    ) -> bool {
        use ControlFlow as CF;
        match error {
            CF::Divergent {
                loc,
                set: _,
                reported,
            } if !*reported => {
                *reported = true;
                let mut diag = diag!(UnusedItem::DeadCode, (*loc, DIVERGENT_MSG));
                if let Some(next_loc) = next_stmt {
                    diag.add_secondary_label((*next_loc, UNREACHABLE_MSG));
                }
                self.add_diag(diag);
                true
            }
            CF::Divergent { .. } | CF::None | CF::Possible => false,
        }
    }

    fn maybe_report_statement_tail_error(
        &mut self,
        error: &mut ControlFlow,
        tail_exp: &T::Exp,
    ) -> bool {
        use ControlFlow as CF;
        if matches!(tail_exp.exp.value, T::UnannotatedExp_::Unit { .. }) {
            match error {
                CF::Divergent {
                    loc,
                    set: _,
                    reported,
                } if !*reported => {
                    *reported = true;
                    self.add_diag(diag!(
                        UnusedItem::TrailingSemi,
                        (tail_exp.exp.loc, SEMI_MSG),
                        (*loc, DIVERGENT_MSG),
                        (tail_exp.exp.loc, INFO_MSG),
                    ));
                    true
                }
                CF::Divergent { .. } | CF::None | CF::Possible => false,
            }
        } else {
            self.maybe_report_statement_error(error, Some(&tail_exp.exp.loc))
        }
    }
}

const VALUE_UNREACHABLE_MSG: &str =
    "Expected a value. Any code surrounding or after this expression will not be reached";

const DIVERGENT_MSG: &str = "Any code after this expression will not be reached";

const UNREACHABLE_MSG: &str =
    "Unreachable code. This statement (and any following statements) will not be executed.";

const SEMI_MSG: &str = "Invalid trailing ';'";
const INFO_MSG: &str =
    "A trailing ';' in an expression block implicitly adds a '()' value after the semicolon. \
     That '()' value will not be reachable";

fn return_called(loc: Loc) -> ControlFlow {
    ControlFlow::Divergent {
        loc,
        set: BTreeSet::from([ControlFlowEntry::ReturnCalled]),
        reported: false,
    }
}

fn abort_called(loc: Loc) -> ControlFlow {
    ControlFlow::Divergent {
        loc,
        set: BTreeSet::from([ControlFlowEntry::AbortCalled]),
        reported: false,
    }
}

fn give_called(loc: Loc, label: BlockLabel) -> ControlFlow {
    ControlFlow::Divergent {
        loc,
        set: BTreeSet::from([ControlFlowEntry::GiveCalled(label)]),
        reported: false,
    }
}

fn continue_called(loc: Loc, label: BlockLabel) -> ControlFlow {
    ControlFlow::Divergent {
        loc,
        set: BTreeSet::from([ControlFlowEntry::ContinueCalled(label)]),
        reported: false,
    }
}

fn infinite_loop(loc: Loc) -> ControlFlow {
    ControlFlow::Divergent {
        loc,
        set: BTreeSet::from([ControlFlowEntry::InfiniteLoop]),
        reported: false,
    }
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(compilation_env: &CompilationEnv, prog: &T::Program) {
    let mut context = Context::new(compilation_env);
    modules(&mut context, &prog.modules);
}

fn modules(context: &mut Context, modules: &UniqueMap<ModuleIdent, T::ModuleDefinition>) {
    for (_, _, mdef) in modules {
        module(context, mdef);
    }
}

fn module(context: &mut Context, mdef: &T::ModuleDefinition) {
    context.push_warning_filter_scope(mdef.warning_filter);
    for (_, cname, cdef) in &mdef.constants {
        constant(context, cname, cdef);
    }
    for (_, fname, fdef) in &mdef.functions {
        function(context, fname, fdef);
    }
    context.pop_warning_filter_scope();
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

fn function(context: &mut Context, _name: &Symbol, f: &T::Function) {
    let T::Function {
        warning_filter,
        body,
        ..
    } = f;
    context.push_warning_filter_scope(*warning_filter);
    function_body(context, body);
    context.pop_warning_filter_scope();
}

fn function_body(context: &mut Context, sp!(_, tb_): &T::FunctionBody) {
    use T::FunctionBody_ as TB;
    if let TB::Defined((_, seq)) = tb_ {
        body(context, seq)
    }
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

fn constant(context: &mut Context, _name: &Symbol, cdef: &T::Constant) {
    context.push_warning_filter_scope(cdef.warning_filter);
    let eloc = cdef.value.exp.loc;
    let tseq = {
        let mut v = VecDeque::new();
        v.push_back(sp(
            eloc,
            T::SequenceItem_::Seq(Box::new(cdef.value.clone())),
        ));
        v
    };
    body(context, &tseq);
    context.pop_warning_filter_scope();
}

//**************************************************************************************************
// Expression Processing
//**************************************************************************************************

// -------------------------------------------------------------------------------------------------
// Tail Position
// -------------------------------------------------------------------------------------------------

fn body(context: &mut Context, seq: &VecDeque<T::SequenceItem>) {
    if !seq.is_empty() {
        tail_block(context, seq);
    }
}

#[growing_stack]
fn tail(context: &mut Context, e: &T::Exp) -> ControlFlow {
    use ControlFlow as CF;
    use T::UnannotatedExp_ as E;
    let T::Exp {
        exp: sp!(eloc, e_), ..
    } = e;

    match e_ {
        // -----------------------------------------------------------------------------------------
        // control flow statements
        // -----------------------------------------------------------------------------------------
        E::IfElse(test, conseq, alt_opt) => do_if(
            context,
            (eloc, test, conseq, alt_opt.as_deref()),
            /* tail_pos */ true,
            tail,
            |context, flow| context.maybe_report_tail_error(flow),
        ),
        E::Match(subject, arms) => do_match(
            context,
            (subject, arms),
            /* tail_pos */ true,
            tail,
            |context, flow| context.maybe_report_tail_error(flow),
        ),
        E::VariantMatch(..) => {
            context.add_diag(ice!((*eloc, "Found variant match in detect_dead_code")));
            CF::None
        }

        // Whiles Loops are treated as statements because they cannot produce values.
        E::While(_, _, _) => statement(context, e),

        // Normal loops are treated as statements. This allows people to write infinite loops in
        // tail positions without error. This is because if it occurs in tail position, we assume
        // it is intentionally infinite. It is, after all not causing any dead code.
        E::Loop { .. } => statement(context, e),

        E::NamedBlock(name, (_, seq)) => {
            // a named block in tail position checks for bad semicolons plus if the body exits that
            // block; if so, at least some of that code is live.
            let body_flow = tail_block(context, seq);
            body_flow.remove_label(name)
        }
        E::Block((_, seq)) => tail_block(context, seq),

        // -----------------------------------------------------------------------------------------
        //  statements
        // -----------------------------------------------------------------------------------------
        E::Return(_) | E::Abort(_) | E::Give(_, _) | E::Continue(_) => value(context, e),
        E::Assign(_, _, _) | E::Mutate(_, _) => CF::None,

        // -----------------------------------------------------------------------------------------
        //  value-like expression
        // -----------------------------------------------------------------------------------------
        _ => value(context, e),
    }
}

fn tail_block(context: &mut Context, seq: &VecDeque<T::SequenceItem>) -> ControlFlow {
    use T::SequenceItem_ as S;
    let last_exp = seq.iter().last();
    let mut stmt_flow = statement_block(
        context, seq, /* stmt_pos */ false, /* skip_last */ true,
    );
    if stmt_flow.is_some() {
        // If we have statement flow and a final expression, we might have an unnecessary
        // semicolon if `last` is just a unit value. Let's check for that.
        if let Some(sp!(_, S::Seq(last))) = last_exp {
            context.maybe_report_statement_tail_error(&mut stmt_flow, last);
            stmt_flow
        } else {
            context.maybe_report_tail_error(&mut stmt_flow);
            stmt_flow
        }
    } else {
        match last_exp {
            None => ControlFlow::None,
            Some(sp!(_, S::Seq(last))) => tail(context, last),
            Some(sp!(loc, _)) => {
                context.add_diag(ice!((
                    *loc,
                    "ICE last sequence item should have been an exp in dead code analysis"
                )));
                ControlFlow::None
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Value Position
// -------------------------------------------------------------------------------------------------

#[growing_stack]
fn value(context: &mut Context, e: &T::Exp) -> ControlFlow {
    use ControlFlow as CF;
    use T::UnannotatedExp_ as E;

    let T::Exp {
        exp: sp!(eloc, e_), ..
    } = e;

    macro_rules! value_report {
        ($nested_value:expr) => {{
            let mut value_flow = value(context, $nested_value);
            if context.maybe_report_value_error(&mut value_flow) {
                return value_flow;
            }
            value_flow
        }};
    }

    match e_ {
        // -----------------------------------------------------------------------------------------
        // control flow statements
        // -----------------------------------------------------------------------------------------
        E::IfElse(test, conseq, alt) => do_if(
            context,
            (eloc, test, conseq, alt.as_deref()),
            /* tail_pos */ false,
            value,
            |context, flow| context.maybe_report_value_error(flow),
        ),
        E::Match(subject, arms) => do_match(
            context,
            (subject, arms),
            /* tail_pos */ false,
            value,
            |context, flow| context.maybe_report_value_error(flow),
        ),
        E::VariantMatch(_subject, _, _arms) => {
            context.add_diag(ice!((*eloc, "Found variant match in detect_dead_code")));
            CF::None
        }
        E::While(..) => statement(context, e),
        E::Loop {
            name,
            body,
            has_break: _,
        } => {
            // A loop can yield values, but only through `break`. We treat the body as a statement,
            // but then consider if it ever breaks out. If it does not, this is an infinite loop.
            let body_flow = statement(context, body);
            let loop_flow = if body_flow.is_none() {
                let mut new_flow = infinite_loop(*eloc);
                context.maybe_report_value_error(&mut new_flow);
                new_flow
            } else {
                body_flow
            };
            loop_flow.remove_label(name)
        }
        E::NamedBlock(name, (_, seq)) => {
            // a named block checks for bad semicolons plus if the body exits that
            // block; if so, at least some of that code is live.
            let body_flow = value_block(context, seq);
            body_flow.remove_label(name)
        }
        E::Block((_, seq)) => value_block(context, seq),

        // -----------------------------------------------------------------------------------------
        //  calls and nested expressions
        // -----------------------------------------------------------------------------------------
        E::ModuleCall(call) => value_report!(&call.arguments),

        E::Builtin(_, args) | E::Vector(_, _, _, args) => value_report!(args),

        E::Pack(_, _, _, fields) | E::PackVariant(_, _, _, _, fields) => {
            let mut flow = CF::None;
            for (_, _, (_, (_, field_exp))) in fields {
                let field_flow = value(context, field_exp);
                flow = flow.combine_seq(field_flow);
                context.maybe_report_value_error(&mut flow);
            }
            flow
        }

        E::ExpList(items) => {
            let mut flow = CF::None;
            for item in items {
                match item {
                    T::ExpListItem::Single(exp, _) => {
                        let item_flow = value(context, exp);
                        flow = flow.combine_seq(item_flow);
                        context.maybe_report_value_error(&mut flow);
                    }
                    T::ExpListItem::Splat(_, _, _) => {
                        context.add_diag(ice!((
                            *eloc,
                            "ICE splat exp unsupported by dead code analysis"
                        )));
                    }
                }
            }
            flow
        }

        E::Annotate(base_exp, _) | E::Cast(base_exp, _) => value(context, base_exp),

        E::Dereference(base_exp)
        | E::UnaryExp(_, base_exp)
        | E::Borrow(_, base_exp, _)
        | E::TempBorrow(_, base_exp) => value_report!(base_exp),

        E::BorrowLocal(_, _) => CF::None,

        // -----------------------------------------------------------------------------------------
        // value-based expressions without subexpressions -- no control flow
        // -----------------------------------------------------------------------------------------
        E::Unit { .. }
        | E::Value(_)
        | E::Constant(_, _)
        | E::Move { .. }
        | E::Copy { .. }
        | E::ErrorConstant { .. } => CF::None,

        // -----------------------------------------------------------------------------------------
        //  statements
        // -----------------------------------------------------------------------------------------
        E::Return(rhs) => value_report!(rhs).combine_seq(return_called(*eloc)),
        E::Abort(rhs) => value_report!(rhs).combine_seq(abort_called(*eloc)),
        E::Give(name, rhs) => value_report!(rhs).combine_seq(give_called(*eloc, *name)),
        E::Continue(name) => continue_called(*eloc, *name),
        E::Assign(_, _, _) | E::Mutate(_, _) => CF::None, // These are unit-valued

        E::BinopExp(_, _, _, _) => process_binops(context, e),

        // -----------------------------------------------------------------------------------------
        // odds and ends
        // -----------------------------------------------------------------------------------------
        E::Use(_) | E::UnresolvedError => CF::None,
    }
}

fn value_block(context: &mut Context, seq: &VecDeque<T::SequenceItem>) -> ControlFlow {
    use T::SequenceItem_ as S;
    let last_exp = seq.iter().last();
    let mut stmt_flow = statement_block(
        context, seq, /* stmt_pos */ false, /* skip_last */ true,
    );
    if stmt_flow.is_some() {
        // If we have statement flow and a final expression, we might have an unnecessary
        // semicolon if `last` is just a unit value. Let's check for that.
        if let Some(sp!(_, S::Seq(last))) = last_exp {
            context.maybe_report_statement_tail_error(&mut stmt_flow, last);
            stmt_flow
        } else {
            context.maybe_report_value_error(&mut stmt_flow);
            stmt_flow
        }
    } else {
        match last_exp {
            None => ControlFlow::None,
            Some(sp!(_, S::Seq(last))) => value(context, last),
            Some(sp!(loc, _)) => {
                context.add_diag(ice!((
                    *loc,
                    "ICE last sequence item should have been an exp in dead code analysis"
                )));
                ControlFlow::None
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Statement Position
// -------------------------------------------------------------------------------------------------

#[growing_stack]
fn statement(context: &mut Context, e: &T::Exp) -> ControlFlow {
    use ControlFlow as CF;
    use T::UnannotatedExp_ as E;
    let T::Exp {
        exp: sp!(eloc, e_), ..
    } = e;

    macro_rules! value_report {
        ($nested_value:expr) => {{
            let mut value_flow = value(context, $nested_value);
            if context.maybe_report_value_error(&mut value_flow) {
                return value_flow;
            }
            value_flow
        }};
    }

    match e_ {
        // -----------------------------------------------------------------------------------------
        // control flow statements
        // -----------------------------------------------------------------------------------------
        // For `if` and `match`, we don't care if the arms individually diverge since we only care
        // about the final, total view of them.
        E::IfElse(test, conseq, alt) => do_if(
            context,
            (eloc, test, conseq, alt.as_deref()),
            /* tail_pos */ false,
            statement,
            |_, _| false,
        ),
        E::Match(subject, arms) => do_match(
            context,
            (subject, arms),
            /* tail_pos */ false,
            statement,
            |_, _| false,
        ),
        E::VariantMatch(_subject, _, _arms) => {
            context.add_diag(ice!((*eloc, "Found variant match in detect_dead_code")));
            CF::None
        }
        E::While(name, test, body) => {
            let mut test_flow = value(context, test);
            if context.maybe_report_value_error(&mut test_flow) {
                test_flow
            } else {
                // Since a body for a While loop is only Possible, not certain, we always force it
                // into Possible.
                let body_flow = statement(context, body).combine_arms(*eloc, CF::None);
                let body_flow = body_flow.remove_label(name);
                test_flow.combine_seq(body_flow)
            }
        }

        E::Loop {
            name,
            body,
            has_break: _,
        } => {
            // A loop can yield values, but only through `break`. We treat the body as a statement,
            // but then consider if it ever breaks out. If it does not, this is an infinite loop.
            let body_flow = statement(context, body);
            let loop_flow = if body_flow.is_none() {
                infinite_loop(*eloc)
            } else {
                // Unlike values, for a loop in statement position we want to preserve if it
                // diverges or not.
                body_flow
            };
            loop_flow.remove_label(name)
        }
        E::NamedBlock(name, (_, seq)) => {
            // a named block checks for bad semicolons plus if the body exits that
            // block; if so, at least some of that code is live.
            let body_flow = statement_block(
                context, seq, /* stmt_pos */ true, /* skip_last */ false,
            );
            body_flow.remove_label(name)
        }
        E::Block((_, seq)) => statement_block(
            context, seq, /* stmt_pos */ true, /* skip_last */ false,
        ),
        E::Return(rhs) => value_report!(rhs).combine_seq(return_called(*eloc)),
        E::Abort(rhs) => value_report!(rhs).combine_seq(abort_called(*eloc)),
        E::Give(name, rhs) => value_report!(rhs).combine_seq(give_called(*eloc, *name)),
        E::Continue(name) => continue_called(*eloc, *name),

        // -----------------------------------------------------------------------------------------
        //  statements with effects
        // -----------------------------------------------------------------------------------------
        E::Assign(_, _, rhs) => value_report!(rhs),
        E::Mutate(lhs, rhs) => value_report!(rhs).combine_seq(value_report!(lhs)),

        // -----------------------------------------------------------------------------------------
        // valued expressions -- when these occur in statement position need their children
        // unravelled to find any embedded, divergent operations.
        // -----------------------------------------------------------------------------------------
        E::ModuleCall(_)
        | E::Builtin(_, _)
        | E::Vector(_, _, _, _)
        | E::Dereference(_)
        | E::UnaryExp(_, _)
        | E::BinopExp(_, _, _, _)
        | E::Pack(_, _, _, _)
        | E::PackVariant(_, _, _, _, _)
        | E::ExpList(_)
        | E::Borrow(_, _, _)
        | E::TempBorrow(_, _)
        | E::Cast(_, _)
        | E::Annotate(_, _)
        | E::BorrowLocal(_, _)
        | E::Constant(_, _)
        | E::ErrorConstant { .. }
        | E::Move { .. }
        | E::Copy { .. }
        | E::UnresolvedError => value(context, e),

        E::Value(_) | E::Unit { .. } => CF::None,

        // -----------------------------------------------------------------------------------------
        // odds and ends -- things we need to deal with but that don't do much
        // -----------------------------------------------------------------------------------------
        E::Use(_) => {
            context.add_diag(ice!((*eloc, "ICE found unexpanded use")));
            CF::None
        }
    }
}

fn statement_block(
    context: &mut Context,
    seq: &VecDeque<T::SequenceItem>,
    stmt_pos: bool,
    skip_last: bool,
) -> ControlFlow {
    use ControlFlow as CF;
    use T::SequenceItem_ as S;

    let seq_has_trailing_unit = has_trailing_unit(seq);
    // if we're in statement position, we need to check for a trailing semicolon error
    // this code does that by noting a trialing unit and then proceeding as if we are not in
    // statement position.
    if stmt_pos && seq_has_trailing_unit {
        let last = seq.iter().last();
        let mut control_flow = statement_block(
            context, seq, /* stmt_pos */ false, /* skip_last */ true,
        );
        if let Some(sp!(_, S::Seq(entry))) = last {
            context.maybe_report_statement_tail_error(&mut control_flow, entry);
        }
        return control_flow;
    }

    // let iterator = if skip_last {
    //     seq.iter().skip_last().enumerate().collect::<Vec<_>>()
    // } else {
    //     seq.iter().enumerate().collect::<Vec<_>>()
    // };
    let last_ndx = usize::saturating_sub(seq.len(), 1);
    let locs: Vec<_> = seq.iter().map(|s| s.loc).collect();

    let mut cur_flow = CF::None;
    for (ndx, sp!(_, seq_item)) in seq.iter().enumerate() {
        if cur_flow.is_divergent() {
            break;
        } else if ndx == last_ndx && skip_last {
        } else {
            match seq_item {
                S::Seq(entry) => {
                    let entry_flow = statement(context, entry);
                    cur_flow = cur_flow.combine_seq(entry_flow);
                    if cur_flow.is_divergent()
                        && ndx != last_ndx
                        && !(ndx + 1 == last_ndx && seq_has_trailing_unit)
                    {
                        context.maybe_report_statement_error(&mut cur_flow, Some(&locs[ndx + 1]));
                    }
                }
                S::Declare(_) => (),
                S::Bind(_, _, rhs) => {
                    let entry_flow = value(context, rhs);
                    cur_flow = cur_flow.combine_seq(entry_flow);
                    context.maybe_report_value_error(&mut cur_flow);
                }
            }
        }
    }
    cur_flow
}

// -------------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------------

fn has_trailing_unit(seq: &VecDeque<T::SequenceItem>) -> bool {
    use T::SequenceItem_ as S;
    if let Some(sp!(_, S::Seq(exp))) = &seq.back() {
        matches!(exp.exp.value, T::UnannotatedExp_::Unit { trailing: true })
    } else {
        false
    }
}

fn do_if<F1, F2>(
    context: &mut Context,
    (loc, test, conseq, alt_opt): (&Loc, &T::Exp, &T::Exp, Option<&T::Exp>),
    tail_pos: bool,
    arm_recur: F1,
    arm_error: F2,
) -> ControlFlow
where
    F1: Fn(&mut Context, &T::Exp) -> ControlFlow,
    F2: Fn(&mut Context, &mut ControlFlow) -> bool,
{
    use ControlFlow as CF;
    let mut value_flow = value(context, test);
    if context.maybe_report_value_error(&mut value_flow) {
        return value_flow;
    };

    let conseq_flow = arm_recur(context, conseq);
    let alt_flow = alt_opt
        .map(|alt| arm_recur(context, alt))
        .unwrap_or(CF::None);
    if tail_pos
        && matches!(conseq.ty, sp!(_, N::Type_::Unit | N::Type_::Anything))
        && matches!(
            alt_opt.map(|alt| &alt.ty),
            None | Some(sp!(_, N::Type_::Unit | N::Type_::Anything))
        )
    {
        return CF::None;
    };
    let mut arms_flow = conseq_flow.combine_arms(*loc, alt_flow);
    if arm_error(context, &mut arms_flow) {
        arms_flow
    } else {
        value_flow.combine_seq(arms_flow)
    }
}

fn do_match<F1, F2>(
    context: &mut Context,
    (subject, arms): (&T::Exp, &Spanned<Vec<T::MatchArm>>),
    tail_pos: bool,
    arm_recur: F1,
    arm_error: F2,
) -> ControlFlow
where
    F1: Fn(&mut Context, &T::Exp) -> ControlFlow,
    F2: Fn(&mut Context, &mut ControlFlow) -> bool,
{
    use ControlFlow as CF;
    let mut subject_flow = value(context, subject);
    if context.maybe_report_value_error(&mut subject_flow) {
        return subject_flow;
    };

    let mut arm_flows = arms
        .value
        .iter()
        .map(|sp!(_, arm)| {
            if let Some(guard) = &arm.guard {
                let mut guard_flow = value(context, guard);
                context.maybe_report_value_error(&mut guard_flow);
            };
            arm_recur(context, &arm.rhs)
        })
        .collect::<Vec<_>>();
    if tail_pos
        && arms.value.iter().all(|arm| {
            matches!(
                arm.value.rhs.ty,
                sp!(_, N::Type_::Unit | N::Type_::Anything)
            )
        })
    {
        return CF::None;
    };
    // We _must_ have at least one arm, but we already produced errors about it.
    let arms_first = if let Some(arm) = arm_flows.pop() {
        arm
    } else {
        CF::None
    };
    let mut arms_flow = arm_flows
        .into_iter()
        .fold(arms_first, |base, arm| base.combine_arms(arms.loc, arm));
    if arm_error(context, &mut arms_flow) {
        arms_flow
    } else {
        subject_flow.combine_seq(arms_flow)
    }
}

//**************************************************************************************************
// Binops
//**************************************************************************************************

fn process_binops(context: &mut Context, e: &T::Exp) -> ControlFlow {
    use T::UnannotatedExp_ as E;

    // ----------------------------------------
    // Convert nested binops into a PN list

    let mut work_queue = vec![e];
    let mut value_stack: Vec<ControlFlow> = vec![];

    while let Some(exp) = work_queue.pop() {
        if let T::Exp {
            exp: sp!(_eloc, E::BinopExp(lhs, sp!(_, op), _, rhs)),
            ..
        } = exp
        {
            match op {
                BinOp_::Or | BinOp_::And => {
                    // We only care about errors in the left-hand side due to laziness
                    work_queue.push(lhs);
                }
                _ => {
                    work_queue.push(rhs);
                    work_queue.push(lhs);
                }
            }
        } else {
            value_stack.push(value(context, exp));
        }
    }

    // ----------------------------------------
    // Now process as an RPN stack

    while let Some(mut flow) = value_stack.pop() {
        if flow.is_divergent() {
            context.maybe_report_value_error(&mut flow);
            return flow;
        }
    }
    ControlFlow::None
}
