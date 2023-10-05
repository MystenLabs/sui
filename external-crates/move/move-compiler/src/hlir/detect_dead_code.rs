// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    expansion::ast::ModuleIdent,
    naming::ast as N,
    parser::ast::BinOp_,
    shared::{unique_map::UniqueMap, *},
    typing::ast as T,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{collections::BTreeMap, iter::Peekable};

//**************************************************************************************************
// Context
//**************************************************************************************************

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ControlFlow_ {
    AlreadyReported,
    AbortCalled,
    Divergent,
    InfiniteLoop,
    LoopControlCalled,
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
            AbortCalled | Divergent | InfiniteLoop | LoopControlCalled | ReturnCalled => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, DIVERGENT_EXP)));
            }
            UnreachableCode => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, DEAD_ERR_CMD)));
            }
            _ => (),
        }
    }

    fn report_tail_error(&mut self, sp!(site, error): ControlFlow) {
        use ControlFlow_::*;
        match error {
            InfiniteLoop => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, DEAD_ERR_CMD)));
            }
            UnreachableCode => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, DEAD_ERR_CMD)));
            }
            _ => (),
        }
    }

    fn report_statement_error(&mut self, sp!(site, error): ControlFlow) {
        use ControlFlow_::*;
        match error {
            AbortCalled | Divergent | InfiniteLoop | ReturnCalled => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, DIVERGENT_EXP)));
            }
            UnreachableCode => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, DEAD_ERR_CMD)));
            }
            _ => (),
        }
    }

    fn report_statement_tail_error(&mut self, sp!(site, error): ControlFlow, tail_exp: &T::Exp) {
        use ControlFlow_::*;
        match error {
            AlreadyReported | AbortCalled | Divergent | InfiniteLoop | ReturnCalled
            | LoopControlCalled
                if matches!(tail_exp.exp.value, T::UnannotatedExp_::Unit { .. }) =>
            {
                self.env.add_diag(diag!(
                    UnusedItem::TrailingSemi,
                    (tail_exp.exp.loc, SEMI_MSG),
                    (site, UNREACHABLE_MSG),
                    (tail_exp.exp.loc, INFO_MSG),
                ));
            }
            AbortCalled | Divergent | InfiniteLoop | ReturnCalled | LoopControlCalled => {
                self.env.add_diag(diag!(
                    UnusedItem::DeadCode,
                    (tail_exp.exp.loc, DEAD_ERR_CMD)
                ));
            }
            UnreachableCode => {
                self.env
                    .add_diag(diag!(UnusedItem::DeadCode, (site, DEAD_ERR_CMD)));
            }
            _ => (),
        }
    }
}

const DIVERGENT_EXP: &str = "Invalid use of a divergent expression. \
     The code following the evaluation of this expression will be dead and should be removed.";

const DEAD_ERR_CMD: &str =
    "Unreachable code. This statement (and any following statements) will not be executed.";

const SEMI_MSG: &str = "Invalid trailing ';'";
const UNREACHABLE_MSG: &str = "Any code after this expression will not be reached";
const INFO_MSG: &str =
    "A trailing ';' in an expression block implicitly adds a '()' value after the semicolon. \
     That '()' value will not be reachable";

fn is_loop_divergent(cf: Option<ControlFlow>) -> bool {
    matches!(cf, Some(sp!(_, ControlFlow_::LoopControlCalled)))
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

fn loop_control_called(loc: Loc) -> Option<ControlFlow> {
    Some(sp(loc, ControlFlow_::LoopControlCalled))
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
    scripts(&mut context, &prog.inner.scripts);
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

fn scripts(context: &mut Context, tscripts: &BTreeMap<Symbol, T::Script>) {
    for sdef in tscripts.values() {
        script(context, sdef);
    }
}

fn script(context: &mut Context, tscript: &T::Script) {
    context
        .env
        .add_warning_filter_scope(tscript.warning_filter.clone());
    for (_, name, cdef) in &tscript.constants {
        constant(context, name, cdef);
    }
    function(context, &tscript.function_name.value(), &tscript.function);
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
    // let signature = function_signature(context, signature);
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
            };
            let conseq_flow = tail(context, conseq);
            let alt_flow = tail(context, alt);
            match (conseq_flow, alt_flow) {
                _ if matches!(ty, sp!(_, N::Type_::Unit)) => None,
                (Some(cflow), Some(aflow)) => {
                    context.report_tail_error(cflow);
                    context.report_tail_error(aflow);
                    None
                }
                _ => None,
            }
        }
        // Whiles and loops Loops are currently moved to statement position
        E::While(_, _) | E::Loop { .. } => statement(context, e),
        E::Block(seq) => tail_block(context, seq),

        // -----------------------------------------------------------------------------------------
        //  statements
        // -----------------------------------------------------------------------------------------
        E::Return(_) => return_called(*eloc),
        E::Abort(_) => abort_called(*eloc),
        E::Break | E::Continue => loop_control_called(*eloc),
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

    let stmt_control_flow = statement_block(context, seq, false, true);

    if let Some(control_flow) = stmt_control_flow {
        if let Some(sp!(_, S::Seq(last))) = last_exp {
            context.report_statement_tail_error(control_flow, last);
            if let Some(tail_control_flow) = tail(context, last) {
                context.report_tail_error(tail_control_flow);
            }
        }
        None
    } else {
        match last_exp {
            None => panic!("ICE tail block with no expressions"),
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

    if is_binop(e) {
        return process_binops(context, e);
    }

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
            };
            let conseq_flow = value(context, conseq);
            let alt_flow = value(context, alt);
            match (conseq_flow, alt_flow) {
                _ if matches!(ty, sp!(_, N::Type_::Unit)) => None,
                (Some(cflow), Some(aflow)) if cflow.value == aflow.value => {
                    context.report_value_error(sp(*eloc, cflow.value));
                    None
                }
                (Some(cflow), Some(aflow)) => {
                    context.report_value_error(cflow);
                    context.report_value_error(aflow);
                    None
                }
                _ => None,
            }
        }
        E::While(_, _) | E::Loop { .. } => statement(context, e),
        E::Block(seq) => value_block(context, seq),

        // -----------------------------------------------------------------------------------------
        //  calls and nested expressions
        // -----------------------------------------------------------------------------------------
        E::ModuleCall(call) => {
            let arg_flows = value_list(context, &call.arguments);
            let result = if arg_flows.is_empty() {
                None
            } else {
                already_reported(*eloc)
            };
            for flow in arg_flows {
                context.report_value_error(flow);
            }
            result
        }

        E::Builtin(_, args) | E::Vector(_, _, _, args) => {
            let arg_flows = value_list(context, args);
            let result = if arg_flows.is_empty() {
                None
            } else {
                already_reported(*eloc)
            };
            for flow in arg_flows {
                context.report_value_error(flow);
            }
            result
        }

        E::Pack(_, _, _, fields) => {
            let mut divergent_term = false;
            for (_, _, (_, (_, field_exp))) in fields {
                if let Some(control_flow) = value(context, field_exp) {
                    context.report_value_error(control_flow);
                    divergent_term = true;
                }
            }
            if divergent_term {
                already_reported(*eloc)
            } else {
                None
            }
        }

        E::ExpList(items) => {
            let mut divergent_term = false;
            for item in items {
                match item {
                    T::ExpListItem::Single(entry, _) => {
                        if let Some(control_flow) = value(context, entry) {
                            context.report_value_error(control_flow);
                            divergent_term = true;
                        }
                    }
                    T::ExpListItem::Splat(_, _, _) => {
                        panic!("ICE splats should be lowered already")
                    }
                }
            }
            if divergent_term {
                already_reported(*eloc)
            } else {
                None
            }
        }

        E::Dereference(base_exp)
        | E::UnaryExp(_, base_exp)
        | E::Borrow(_, base_exp, _)
        | E::Cast(base_exp, _)
        | E::TempBorrow(_, base_exp) => {
            if let Some(control_flow) = value(context, base_exp) {
                context.report_value_error(control_flow);
            }
            None
        }
        E::BorrowLocal(_, _) => None,
        E::Annotate(base_exp, _) => value(context, base_exp),

        // -----------------------------------------------------------------------------------------
        // value-based expressions without subexpressions -- no control flow
        // -----------------------------------------------------------------------------------------
        E::Unit { .. } | E::Value(_) | E::Constant(_, _) | E::Move { .. } | E::Copy { .. } => None,

        // -----------------------------------------------------------------------------------------
        //  statements
        // -----------------------------------------------------------------------------------------
        E::Return(_) => return_called(*eloc),
        E::Abort(_) => abort_called(*eloc),
        E::Break | E::Continue => loop_control_called(*eloc),
        E::Assign(_, _, _) | E::Mutate(_, _) => None, // These are unit-valued

        // -----------------------------------------------------------------------------------------
        //  matches that handled earlier
        // -----------------------------------------------------------------------------------------
        E::BinopExp(_, _, _, _) => panic!("ICE binops unhandled"),

        // -----------------------------------------------------------------------------------------
        // odds and ends
        // -----------------------------------------------------------------------------------------
        E::Use(_) | E::Spec(..) | E::UnresolvedError => None,
    }
}

fn value_block(context: &mut Context, seq: &T::Sequence) -> Option<ControlFlow> {
    use T::SequenceItem_ as S;
    let last_exp = seq.iter().last();
    if let Some(control_flow) = statement_block(context, seq, false, true) {
        if let Some(sp!(_, S::Seq(last))) = last_exp {
            context.report_statement_tail_error(control_flow, last);
            value(context, last)
        } else {
            context.report_value_error(control_flow);
            None
        }
    } else {
        match last_exp {
            None => None,
            Some(sp!(_, S::Seq(last))) => value(context, last),
            Some(_) => panic!("ICE last sequence item should be an exp"),
        }
    }
}

fn value_list(context: &mut Context, e: &T::Exp) -> Vec<ControlFlow> {
    use T::UnannotatedExp_ as TE;
    let mut result = vec![];
    if let TE::ExpList(ref items) = e.exp.value {
        for item in items {
            match item {
                T::ExpListItem::Single(exp, _) => {
                    if let Some(control_flow) = value(context, exp) {
                        result.push(control_flow);
                    }
                }
                T::ExpListItem::Splat(_, _, _) => panic!("ICE spalt is unsupported."),
            }
        }
    } else if let Some(control_flow) = value(context, e) {
        result.push(control_flow)
    }
    result
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
                Some(sp(*eloc, test_control_flow.value))
            } else {
                match (statement(context, conseq), statement(context, alt)) {
                    (Some(cflow), Some(aflow)) if (cflow.value == aflow.value) => {
                        Some(sp(*eloc, cflow.value))
                    }
                    (Some(_), Some(_)) => divergent(*eloc),
                    _ => None,
                }
            }
        }

        E::While(test, body) => {
            if let Some(test_control_flow) = value(context, test) {
                context.report_value_error(test_control_flow);
                statement(context, body);
                Some(test_control_flow)
            } else {
                statement(context, body);
                // we don't know if a while loop will ever run so we drop errors for the bodies.
                None
            }
        }

        E::Loop { body, has_break } => {
            let body_result = statement(context, body);
            if !has_break {
                infinite_loop(*eloc)
            } else if is_loop_divergent(body_result) || *has_break {
                // if the loop has a break, only Godel knows if it'll call it
                None
            } else {
                body_result
            }
        }
        E::Block(seq) => statement_block(context, seq, true, false),
        E::Return(rhs) => {
            if let Some(rhs_control_flow) = value(context, rhs) {
                context.report_value_error(rhs_control_flow);
            }
            return_called(*eloc)
        }
        E::Abort(rhs) => {
            if let Some(rhs_control_flow) = value(context, rhs) {
                context.report_value_error(rhs_control_flow);
            }
            abort_called(*eloc)
        }
        E::Break | E::Continue => loop_control_called(*eloc),

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
            }
            if let Some(lhs_control_flow) = value(context, lhs) {
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
        | E::ExpList(_)
        | E::Borrow(_, _, _)
        | E::TempBorrow(_, _)
        | E::Cast(_, _)
        | E::Annotate(_, _)
        | E::BorrowLocal(_, _)
        | E::Constant(_, _)
        | E::Move { .. }
        | E::Copy { .. }
        | E::Spec(..)
        | E::UnresolvedError => value_statement(context, e),

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

    // special case trailing semicolon reporting
    if stmt_pos && has_trailing_unit(seq) {
        let last = seq.iter().last();
        let result = statement_block(context, seq, false, true);
        if let (Some(error), Some(sp!(_, S::Seq(entry)))) = (result, last) {
            context.report_statement_tail_error(error, entry);
            return Some(error);
        } else {
            return result;
        }
    }

    let mut saw_error = false;

    let iterator = if skip_last {
        seq.iter().skip_last().enumerate().collect::<Vec<_>>()
    } else {
        seq.iter().enumerate().collect::<Vec<_>>()
    };
    let last_ndx = iterator.iter().skip(1).len();
    let locs: Vec<_> = iterator.iter().map(|(_, s)| s.loc).collect();

    for (ndx, sp!(_, seq_item)) in iterator {
        match seq_item {
            S::Seq(entry) if saw_error => {
                // in an error mode, we process for any more errors that self-report.
                statement(context, entry);
            }
            S::Seq(entry) if ndx == last_ndx => {
                return statement(context, entry);
            }
            S::Seq(entry) => {
                let entry_result = statement(context, entry);
                if entry_result.is_some() {
                    context.report_statement_error(unreachable_code(locs[ndx + 1]).unwrap());
                    saw_error = true;
                }
            }
            S::Declare(_) => (),
            S::Bind(_, _, expr) => {
                if let Some(control_flow) = value(context, expr) {
                    context.report_value_error(control_flow);
                }
            }
        }
    }
    None
}

fn value_statement(context: &mut Context, e: &T::Exp) -> Option<ControlFlow> {
    if let Some(control_flow) = value(context, e) {
        context.report_value_error(control_flow);
        Some(control_flow)
    } else {
        None
    }
}

// -------------------------------------------------------------------------------------------------
// HHelpers
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

fn is_binop(e: &T::Exp) -> bool {
    use T::UnannotatedExp_ as E;
    matches!(e.exp.value, E::BinopExp(_, _, _, _))
}

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
                }
                if let Some(control_flow) = rhs {
                    context.report_value_error(control_flow);
                }
                let new_value_flow = match (lhs, rhs) {
                    (Some(_), _) => already_reported(eloc),
                    (_, Some(_)) => already_reported(eloc),
                    _ => None,
                };
                value_stack.push(new_value_flow);
            }
            Pn::Val(maybe_control_flow) => value_stack.push(maybe_control_flow),
        }
    }
    value_stack.pop().unwrap()
}
