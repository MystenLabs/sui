// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    expansion::ast::ModuleIdent,
    naming::ast::{self as N, BlockLabel},
    parser::ast::BinOp_,
    shared::{unique_map::UniqueMap, *},
    typing::ast as T,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::iter::Peekable;

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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ControlFlow_ {
    AlreadyReported,
    AbortCalled,
    Divergent,
    InfiniteLoop,
    NamedBlockControlCalled(BlockLabel), // tracks the name
    ReturnCalled,
    UnreachableCode,
}

type ControlFlow = Spanned<ControlFlow_>;

struct Context<'env> {
    env: &'env mut CompilationEnv,
}

impl<'env> Context<'env> {
    pub fn new(env: &'env mut CompilationEnv) -> Self {
        Context { env }
    }

    fn report_value_error(&mut self, sp!(site, error): ControlFlow) {
        use ControlFlow_::*;
        match error {
            AbortCalled | Divergent | InfiniteLoop | NamedBlockControlCalled(_) | ReturnCalled => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, VALUE_UNREACHABLE_MSG)));
            }
            UnreachableCode => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, NOT_EXECUTED_MSG)));
            }
            _ => (),
        }
    }

    fn report_tail_error(&mut self, sp!(site, error): ControlFlow) {
        use ControlFlow_::*;
        match error {
            AbortCalled | InfiniteLoop => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, NOT_EXECUTED_MSG)));
            }
            UnreachableCode => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, NOT_EXECUTED_MSG)));
            }
            _ => (),
        }
    }

    fn report_statement_error(&mut self, sp!(site, error): ControlFlow) {
        use ControlFlow_::*;
        match error {
            AbortCalled | Divergent | InfiniteLoop | ReturnCalled => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, UNREACHABLE_MSG)));
            }
            UnreachableCode => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, NOT_EXECUTED_MSG)));
            }
            _ => (),
        }
    }

    fn report_statement_tail_error(&mut self, sp!(site, error): ControlFlow, tail_exp: &T::Exp) {
        use ControlFlow_::*;
        match error {
            AbortCalled | Divergent | InfiniteLoop | ReturnCalled | NamedBlockControlCalled(_)
                if matches!(tail_exp.exp.value, T::UnannotatedExp_::Unit { .. }) =>
            {
                self.env.add_diag(diag!(
                    UnusedItem::TrailingSemi,
                    (tail_exp.exp.loc, SEMI_MSG),
                    (site, UNREACHABLE_MSG),
                    (tail_exp.exp.loc, INFO_MSG),
                ));
            }
            AbortCalled | Divergent | InfiniteLoop | ReturnCalled | NamedBlockControlCalled(_) => {
                self.env.add_diag(diag!(
                    UnusedItem::DeadCode,
                    (tail_exp.exp.loc, NOT_EXECUTED_MSG)
                ));
            }
            UnreachableCode => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, NOT_EXECUTED_MSG)));
            }
            _ => (),
        }
    }
}

const VALUE_UNREACHABLE_MSG: &str =
    "Expected a value. Any code surrounding or after this expression will not be reached";

const UNREACHABLE_MSG: &str = "Any code after this expression will not be reached";

const NOT_EXECUTED_MSG: &str =
    "Unreachable code. This statement (and any following statements) will not be executed.";

const SEMI_MSG: &str = "Invalid trailing ';'";
const INFO_MSG: &str =
    "A trailing ';' in an expression block implicitly adds a '()' value after the semicolon. \
     That '()' value will not be reachable";

fn exits_named_block(name: BlockLabel, cf: Option<ControlFlow>) -> bool {
    match cf {
        Some(sp!(_, ControlFlow_::NamedBlockControlCalled(break_name))) => name == break_name,
        _ => false,
    }
}

fn already_reported(loc: Loc) -> Option<ControlFlow> {
    Some(sp(loc, ControlFlow_::AlreadyReported))
}

fn abort_called(loc: Loc) -> Option<ControlFlow> {
    Some(sp(loc, ControlFlow_::AbortCalled))
}

// catch all for when we have to combine failures
fn divergent(loc: Loc) -> Option<ControlFlow> {
    Some(sp(loc, ControlFlow_::Divergent))
}

fn infinite_loop(loc: Loc) -> Option<ControlFlow> {
    Some(sp(loc, ControlFlow_::InfiniteLoop))
}

fn named_control_called(loop_name: BlockLabel, loc: Loc) -> Option<ControlFlow> {
    Some(sp(loc, ControlFlow_::NamedBlockControlCalled(loop_name)))
}

fn return_called(loc: Loc) -> Option<ControlFlow> {
    Some(sp(loc, ControlFlow_::ReturnCalled))
}

fn unreachable_code(loc: Loc) -> Option<ControlFlow> {
    Some(sp(loc, ControlFlow_::UnreachableCode))
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(compilation_env: &mut CompilationEnv, prog: &T::Program) {
    let mut context = Context::new(compilation_env);
    modules(&mut context, &prog.inner.modules);
}

fn modules(context: &mut Context, modules: &UniqueMap<ModuleIdent, T::ModuleDefinition>) {
    for (_, _, mdef) in modules {
        module(context, mdef);
    }
}

fn module(context: &mut Context, mdef: &T::ModuleDefinition) {
    context
        .env
        .add_warning_filter_scope(mdef.warning_filter.clone());
    for (_, cname, cdef) in &mdef.constants {
        constant(context, cname, cdef);
    }
    for (_, fname, fdef) in &mdef.functions {
        function(context, fname, fdef);
    }
    context.env.pop_warning_filter_scope();
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
    context.env.add_warning_filter_scope(warning_filter.clone());
    function_body(context, body);
    context.env.pop_warning_filter_scope();
}

fn function_body(context: &mut Context, sp!(_, tb_): &T::FunctionBody) {
    use T::FunctionBody_ as TB;
    if let TB::Defined(seq) = tb_ {
        body(context, seq)
    }
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

fn constant(context: &mut Context, _name: &Symbol, cdef: &T::Constant) {
    context
        .env
        .add_warning_filter_scope(cdef.warning_filter.clone());
    let eloc = cdef.value.exp.loc;
    let tseq = {
        let mut v = T::Sequence::new();
        v.push_back(sp(
            eloc,
            T::SequenceItem_::Seq(Box::new(cdef.value.clone())),
        ));
        v
    };
    body(context, &tseq);
    context.env.pop_warning_filter_scope();
}

//**************************************************************************************************
// Expression Processing
//**************************************************************************************************

// -------------------------------------------------------------------------------------------------
// Tail Position
// -------------------------------------------------------------------------------------------------

fn body(context: &mut Context, seq: &T::Sequence) {
    if !seq.is_empty() {
        tail_block(context, seq);
    }
}

fn tail(context: &mut Context, e: &T::Exp) -> Option<ControlFlow> {
    use T::UnannotatedExp_ as E;
    let T::Exp {
        ty,
        exp: sp!(eloc, e_),
    } = e;

    match e_ {
        // -----------------------------------------------------------------------------------------
        // control flow statements
        // -----------------------------------------------------------------------------------------
        E::IfElse(test, conseq, alt) => {
            if let Some(test_control_flow) = value(context, test) {
                context.report_value_error(test_control_flow);
                return None;
            };
            let conseq_flow = tail(context, conseq);
            let alt_flow = tail(context, alt);
            match (conseq_flow, alt_flow) {
                _ if matches!(ty, sp!(_, N::Type_::Unit)) => None,
                (Some(cflow), Some(aflow)) => {
                    if cflow.value == aflow.value {
                        context.report_tail_error(sp(*eloc, cflow.value));
                    } else {
                        context.report_tail_error(cflow);
                        context.report_tail_error(aflow);
                    }
                    None
                }
                _ => None,
            }
        }
        E::Match(subject, arms) => {
            if let Some(test_control_flow) = value(context, subject) {
                context.report_value_error(test_control_flow);
                return None;
            };
            let arm_somes = arms
                .value
                .iter()
                .map(|sp!(_, arm)| tail(context, &arm.rhs))
                .collect::<Vec<_>>();
            if arm_somes.iter().all(|arm_opt| arm_opt.is_some()) {
                for arm_opt in arm_somes {
                    let sp!(aloc, arm_error) = arm_opt.unwrap();
                    context.report_tail_error(sp(aloc, arm_error))
                }
            }
            None
        }
        E::VariantMatch(..) => panic!("ICE should not have a variant match in this position."),

        // Whiles and loops Loops are currently moved to statement position
        E::While(_, _, _) | E::Loop { .. } => statement(context, e),
        E::NamedBlock(name, seq) => {
            // a named block in tail position checks for bad semicolons plus if the body exits that
            // block; if so, at least some of that code is live.
            let body_result = tail_block(context, seq);
            if exits_named_block(*name, body_result) {
                None
            } else {
                body_result
            }
        }
        E::Block(seq) => tail_block(context, seq),

        // -----------------------------------------------------------------------------------------
        //  statements
        // -----------------------------------------------------------------------------------------
        E::Return(_) | E::Abort(_) | E::Give(_, _) | E::Continue(_) => value(context, e),
        E::Assign(_, _, _) | E::Mutate(_, _) => None,

        // -----------------------------------------------------------------------------------------
        //  value-like expression
        // -----------------------------------------------------------------------------------------
        _ => value(context, e),
    }
}

fn tail_block(context: &mut Context, seq: &T::Sequence) -> Option<ControlFlow> {
    use T::SequenceItem_ as S;
    let last_exp = seq.iter().last();
    let stmt_flow = statement_block(
        context, seq, /* stmt_pos */ false, /* skip_last */ true,
    );
    if let (Some(control_flow), Some(sp!(_, S::Seq(last)))) = (stmt_flow, last_exp) {
        context.report_statement_tail_error(control_flow, last);
        None
    } else if let Some(control_flow) = stmt_flow {
        context.report_tail_error(control_flow);
        None
    } else {
        match last_exp {
            None => None,
            Some(sp!(_, S::Seq(last))) => tail(context, last),
            Some(_) => panic!("ICE last sequence item should be an exp"),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Value Position
// -------------------------------------------------------------------------------------------------

fn value(context: &mut Context, e: &T::Exp) -> Option<ControlFlow> {
    use T::UnannotatedExp_ as E;

    let T::Exp {
        exp: sp!(eloc, e_), ..
    } = e;

    macro_rules! value_report {
        ($nested_value:expr) => {{
            if let Some(control_flow) = value(context, $nested_value) {
                context.report_value_error(control_flow);
                already_reported(*eloc)
            } else {
                None
            }
        }};
    }

    match e_ {
        // -----------------------------------------------------------------------------------------
        // control flow statements
        // -----------------------------------------------------------------------------------------
        E::IfElse(test, conseq, alt) => {
            if let Some(test_control_flow) = value(context, test) {
                context.report_value_error(test_control_flow);
                return already_reported(*eloc);
            };
            if let (Some(cflow), Some(aflow)) = (value(context, conseq), value(context, alt)) {
                if cflow.value == aflow.value {
                    context.report_value_error(sp(*eloc, cflow.value));
                } else {
                    context.report_value_error(cflow);
                    context.report_value_error(aflow);
                }
                return already_reported(*eloc);
            }
            None
        }
        E::Match(subject, arms) => {
            if let Some(test_control_flow) = value(context, subject) {
                context.report_value_error(test_control_flow);
                return None;
            };
            let arm_somes = arms
                .value
                .iter()
                .map(|sp!(_, arm)| value(context, &arm.rhs))
                .collect::<Vec<_>>();
            if arm_somes.iter().all(|arm_opt| arm_opt.is_some()) {
                for arm_opt in arm_somes {
                    let sp!(aloc, arm_error) = arm_opt.unwrap();
                    context.report_value_error(sp(aloc, arm_error))
                }
            }
            None
        }
        E::VariantMatch(..) => panic!("ICE should not have a variant match in this position."),
        E::While(..) | E::Loop { .. } => statement(context, e),
        E::NamedBlock(name, seq) => {
            // a named block in value position checks if the body exits that block; if so, at least
            // some of that code is live.
            let body_result = value_block(context, seq);
            if exits_named_block(*name, body_result) {
                None
            } else {
                body_result
            }
        }
        E::Block(seq) => value_block(context, seq),

        // -----------------------------------------------------------------------------------------
        //  calls and nested expressions
        // -----------------------------------------------------------------------------------------
        E::ModuleCall(call) => value_report!(&call.arguments),

        E::Builtin(_, args) | E::Vector(_, _, _, args) => value_report!(args),

        E::Pack(_, _, _, fields) => fields
            .iter()
            .find_map(|(_, _, (_, (_, field_exp)))| value_report!(field_exp)),

        E::PackVariant(_, _, _, _, fields) => fields
            .iter()
            .find_map(|(_, _, (_, (_, field_exp)))| value_report!(field_exp)),

        E::ExpList(_) => {
            use T::UnannotatedExp_ as TE;
            if let TE::ExpList(items) = &e.exp.value {
                for item in items {
                    match item {
                        T::ExpListItem::Single(exp, _) => {
                            let next = value_report!(exp);
                            if next.is_some() {
                                return next;
                            }
                        }
                        T::ExpListItem::Splat(_, _, _) => panic!("ICE splat is unsupported."),
                    }
                }
                None
            } else {
                value_report!(e)
            }
        }

        E::Annotate(base_exp, _)
        | E::Dereference(base_exp)
        | E::UnaryExp(_, base_exp)
        | E::Borrow(_, base_exp, _)
        | E::Cast(base_exp, _)
        | E::TempBorrow(_, base_exp) => value_report!(base_exp),

        E::BorrowLocal(_, _) => None,

        // -----------------------------------------------------------------------------------------
        // value-based expressions without subexpressions -- no control flow
        // -----------------------------------------------------------------------------------------
        E::Unit { .. } | E::Value(_) | E::Constant(_, _) | E::Move { .. } | E::Copy { .. } => None,

        // -----------------------------------------------------------------------------------------
        //  statements
        // -----------------------------------------------------------------------------------------
        E::Return(rhs) => value_report!(rhs).or_else(|| return_called(*eloc)),
        E::Abort(rhs) => value_report!(rhs).or_else(|| abort_called(*eloc)),
        E::Give(name, rhs) => value_report!(rhs).or_else(|| named_control_called(*name, *eloc)),
        E::Continue(name) => named_control_called(*name, *eloc),
        E::Assign(_, _, _) | E::Mutate(_, _) => None, // These are unit-valued

        E::BinopExp(_, _, _, _) => process_binops(context, e),

        // -----------------------------------------------------------------------------------------
        // odds and ends
        // -----------------------------------------------------------------------------------------
        E::Use(_) | E::UnresolvedError => None,
    }
}

fn value_block(context: &mut Context, seq: &T::Sequence) -> Option<ControlFlow> {
    use T::SequenceItem_ as S;
    let last_exp = seq.iter().last();
    let stmt_flow = statement_block(
        context, seq, /* stmt_pos */ false, /* skip_last */ true,
    );
    if let (Some(control_flow), Some(sp!(_, S::Seq(last)))) = (stmt_flow, last_exp) {
        context.report_statement_tail_error(control_flow, last);
        already_reported(control_flow.loc)
    } else if let Some(control_flow) = stmt_flow {
        context.report_value_error(control_flow);
        already_reported(control_flow.loc)
    } else {
        match last_exp {
            None => None,
            Some(sp!(_, S::Seq(last))) => value(context, last),
            Some(_) => panic!("ICE last sequence item should be an exp"),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Statement Position
// -------------------------------------------------------------------------------------------------

fn statement(context: &mut Context, e: &T::Exp) -> Option<ControlFlow> {
    use T::UnannotatedExp_ as E;
    let T::Exp {
        exp: sp!(eloc, e_), ..
    } = e;
    match e_ {
        // -----------------------------------------------------------------------------------------
        // control flow statements
        // -----------------------------------------------------------------------------------------
        E::IfElse(test, conseq, alt) => {
            if let Some(test_control_flow) = value(context, test) {
                context.report_value_error(test_control_flow);
                statement(context, conseq);
                statement(context, alt);
                already_reported(*eloc)
            } else {
                // if the test was okay but the arms both diverged, we need to report that for the
                // purpose of trailing semicolons.
                match (statement(context, conseq), statement(context, alt)) {
                    (Some(_), Some(_)) => divergent(*eloc),
                    _ => None,
                }
            }
        }
        E::Match(subject, arms) => {
            if let Some(test_control_flow) = value(context, subject) {
                context.report_value_error(test_control_flow);
                for sp!(_, arm) in arms.value.iter() {
                    statement(context, &arm.rhs);
                }
                already_reported(*eloc)
            } else {
                // if the test was okay but all arms both diverged, we need to report that for the
                // purpose of trailing semicolons.
                let arm_somes = arms
                    .value
                    .iter()
                    .map(|sp!(_, arm)| statement(context, &arm.rhs))
                    .collect::<Vec<_>>();
                if arm_somes.iter().all(|arm_opt| arm_opt.is_some()) {
                    divergent(*eloc)
                } else {
                    None
                }
            }
        }
        E::VariantMatch(..) => panic!("ICE should not have a variant match in this position."),

        E::While(test, _, body) => {
            if let Some(test_control_flow) = value(context, test) {
                context.report_value_error(test_control_flow);
                already_reported(*eloc)
            } else {
                statement(context, body);
                // we don't know if a while loop will ever run so we drop errors for the bodies.
                None
            }
        }

        E::Loop {
            name,
            body,
            has_break,
        } => {
            let body_result = statement(context, body);
            if !has_break {
                infinite_loop(*eloc)
            } else if exits_named_block(*name, body_result) || *has_break {
                // if the loop has a break, only Godel knows if it'll call it
                None
            } else {
                body_result
            }
        }
        E::NamedBlock(name, seq) => {
            // a named block in statement position checks if the body exits that block; if so, at
            // least some of that code is live.
            let body_result = value_block(context, seq);
            if exits_named_block(*name, body_result) {
                None
            } else {
                body_result
            }
        }
        E::Block(seq) => statement_block(
            context, seq, /* stmt_pos */ true, /* skip_last */ false,
        ),
        E::Return(rhs) => {
            if let Some(rhs_control_flow) = value(context, rhs) {
                context.report_value_error(rhs_control_flow);
                already_reported(*eloc)
            } else {
                return_called(*eloc)
            }
        }
        E::Abort(rhs) => {
            if let Some(rhs_control_flow) = value(context, rhs) {
                context.report_value_error(rhs_control_flow);
                already_reported(*eloc)
            } else {
                abort_called(*eloc)
            }
        }
        E::Give(name, _) | E::Continue(name) => named_control_called(*name, *eloc),

        // -----------------------------------------------------------------------------------------
        //  statements with effects
        // -----------------------------------------------------------------------------------------
        E::Assign(_, _, rhs) => {
            if let Some(rhs_control_flow) = value(context, rhs) {
                context.report_value_error(rhs_control_flow);
            }
            None
        }
        E::Mutate(lhs, rhs) => {
            if let Some(rhs_control_flow) = value(context, rhs) {
                context.report_value_error(rhs_control_flow);
            } else if let Some(lhs_control_flow) = value(context, lhs) {
                context.report_value_error(lhs_control_flow);
            }
            None
        }

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
        | E::Move { .. }
        | E::Copy { .. }
        | E::UnresolvedError => value(context, e),

        E::Value(_) | E::Unit { .. } => None,

        // -----------------------------------------------------------------------------------------
        // odds and ends -- things we need to deal with but that don't do much
        // -----------------------------------------------------------------------------------------
        E::Use(_) => panic!("ICE unexpanded use"),
    }
}

fn statement_block(
    context: &mut Context,
    seq: &T::Sequence,
    stmt_pos: bool,
    skip_last: bool,
) -> Option<ControlFlow> {
    use T::SequenceItem_ as S;

    // if we're in statement position, we need to check for a trailing semicolon error
    // this code does that by noting a trialing unit and then proceeding as if we are not in
    // statement position.
    if stmt_pos && has_trailing_unit(seq) {
        let last = seq.iter().last();
        let result = statement_block(
            context, seq, /* stmt_pos */ false, /* skip_last */ true,
        );
        return if let (Some(control_flow), Some(sp!(_, S::Seq(entry)))) = (result, last) {
            context.report_statement_tail_error(control_flow, entry);
            None
        } else {
            None
        };
    }

    let iterator = if skip_last {
        seq.iter().skip_last().enumerate().collect::<Vec<_>>()
    } else {
        seq.iter().enumerate().collect::<Vec<_>>()
    };
    let last_ndx = usize::saturating_sub(iterator.len(), 1);
    let locs: Vec<_> = iterator.iter().map(|(_, s)| s.loc).collect();

    for (ndx, sp!(_, seq_item)) in iterator {
        match seq_item {
            S::Seq(entry) if ndx == last_ndx => {
                // If this is the last statement, the error may indicate a trailing semicolon
                // error. Return it to whoever is expecting it so they can report it appropriately.
                return statement(context, entry);
            }
            S::Seq(entry) => {
                let entry_result = statement(context, entry);
                if entry_result.is_some() {
                    context.report_statement_error(unreachable_code(locs[ndx + 1]).unwrap());
                    return None;
                }
            }
            S::Declare(_) => (),
            S::Bind(_, _, expr) => {
                if let Some(control_flow) = value(context, expr) {
                    context.report_value_error(control_flow);
                    return None;
                }
            }
        }
    }
    None
}

// -------------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------------

struct SkipLastIterator<I: Iterator>(Peekable<I>);
impl<I: Iterator> Iterator for SkipLastIterator<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.0.next();
        self.0.peek().map(|_| item.unwrap())
    }
}
trait SkipLast: Iterator + Sized {
    fn skip_last(self) -> SkipLastIterator<Self> {
        SkipLastIterator(self.peekable())
    }
}
impl<I: Iterator> SkipLast for I {}

fn has_trailing_unit(seq: &T::Sequence) -> bool {
    use T::SequenceItem_ as S;
    if let Some(sp!(_, S::Seq(exp))) = &seq.back() {
        matches!(exp.exp.value, T::UnannotatedExp_::Unit { trailing: true })
    } else {
        false
    }
}

//**************************************************************************************************
// Binops
//**************************************************************************************************

fn process_binops(context: &mut Context, e: &T::Exp) -> Option<ControlFlow> {
    use T::UnannotatedExp_ as E;

    enum Pn {
        Op(BinOp_, Loc),
        Val(Option<ControlFlow>),
    }

    // ----------------------------------------
    // Convert nested binops into a PN list

    let mut pn_stack = vec![];

    let mut work_queue = vec![e];

    while let Some(exp) = work_queue.pop() {
        if let T::Exp {
            exp: sp!(eloc, E::BinopExp(lhs, sp!(_, op), _, rhs)),
            ..
        } = exp
        {
            pn_stack.push(Pn::Op(*op, *eloc));
            // push on backwards so when we reverse the stack, we are in RPN order
            work_queue.push(rhs);
            work_queue.push(lhs);
        } else {
            pn_stack.push(Pn::Val(value(context, exp)));
        }
    }

    // ----------------------------------------
    // Now process as an RPN stack

    let mut value_stack: Vec<Option<ControlFlow>> = vec![];

    for entry in pn_stack.into_iter().rev() {
        match entry {
            Pn::Op(BinOp_::And, _) | Pn::Op(BinOp_::Or, _) => {
                let test = value_stack.pop().expect("ICE binop hlir issue");
                let _rhs = value_stack.pop().expect("ICE binop hlir issue");
                // we only care about errors in the test, as the rhs is lazy
                value_stack.push(test);
            }
            Pn::Op(_, eloc) => {
                let lhs = value_stack.pop().expect("ICE binop hlir issue");
                let rhs = value_stack.pop().expect("ICE binop hlir issue");
                if let Some(control_flow) = lhs {
                    context.report_value_error(control_flow);
                    value_stack.push(already_reported(eloc));
                } else if let Some(control_flow) = rhs {
                    context.report_value_error(control_flow);
                    value_stack.push(already_reported(eloc));
                } else {
                    value_stack.push(lhs);
                }
            }
            Pn::Val(maybe_control_flow) => value_stack.push(maybe_control_flow),
        }
    }
    value_stack.pop().unwrap()
}
