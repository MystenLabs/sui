// Copyright (c) Verichains, 2023

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use anyhow::Ok;
use move_model::model::{FunctionEnv, ModuleId};
use move_stackless_bytecode::stackless_bytecode::{AssignKind, Bytecode};

use expr_node::*;
use operations_evaluator::*;

pub mod expr_node;
pub mod operations_evaluator;

#[derive(Clone, Debug)]
pub struct ReturnValueHint {
    pub ty: move_model::ty::Type,
}

pub struct StacklessEvaluationRunResult {
    pub results: Expr,
    pub new_variables: HashSet<usize>,
    pub flushed_variables: HashSet<usize>,
    pub cannot_keep_as_expr: bool,
}

#[derive(Clone, Debug)]
struct VariableValueSnapshot {
    value: Expr,
    defined_in_context: Option<usize>,
    assignment_idx: usize,
}

#[derive(Debug)]
pub struct StacklessEvaluationContext<'a> {
    context_id: usize,
    assignment_id_provider: Rc<RefCell<usize>>,
    variables: HashMap<usize, VariableValueSnapshot>,
    pending_variables: HashMap<usize, VariableValueSnapshot>,
    finalized_pending_variables: HashSet<usize>,
    func_env: &'a FunctionEnv<'a>,
    last_branch_expr: Option<Expr>,
    loop_entry: bool,
    predefined_aliases: HashMap<usize, usize>,
}

impl<'a> Clone for StacklessEvaluationContext<'a> {
    fn clone(&self) -> Self {
        let next_id = self.next_assignment_id();
        Self {
            context_id: next_id,
            assignment_id_provider: self.assignment_id_provider.clone(),
            variables: self.variables.clone(),
            pending_variables: self.pending_variables.clone(),
            finalized_pending_variables: self.finalized_pending_variables.clone(),
            func_env: self.func_env,
            last_branch_expr: self.last_branch_expr.clone(),
            predefined_aliases: self.predefined_aliases.clone(),
            // this property is not cloned
            loop_entry: false,
        }
    }
}

impl<'a> StacklessEvaluationContext<'a> {
    pub fn new(func_env: &'a FunctionEnv<'a>) -> Self {
        Self {
            context_id: 1,
            variables: HashMap::new(),
            pending_variables: HashMap::new(),
            finalized_pending_variables: HashSet::new(),
            assignment_id_provider: Rc::new(RefCell::new(0)),
            func_env,
            last_branch_expr: None,
            loop_entry: false,
            predefined_aliases: HashMap::new(),
        }
    }

    pub fn shortest_prefix(&self, mod_id: &ModuleId) -> String {
        super::super::utils::shortest_prefix(&self.func_env.module_env, mod_id)
    }

    pub fn defined(&self, idx: usize) -> bool {
        self.variables.contains_key(&idx)
    }

    pub fn defined_or_pending(&self, idx: usize) -> bool {
        self.variables.contains_key(&idx) || self.pending_variables.contains_key(&idx)
    }

    fn get_pending_var(&self, idx: usize) -> Option<&VariableValueSnapshot> {
        if let Some(pending) = self.pending_variables.get(&idx) {
            Some(pending)
        } else {
            if let Some(alias) = self.predefined_aliases.get(&idx) {
                self.get_pending_var(*alias)
            } else {
                None
            }
        }
    }

    pub fn get_var_with_allow_undefined(&self, idx: usize, allow_undefined: bool) -> Expr {
        if !self.is_flushed(idx) {
            if let Some(VariableValueSnapshot {
                value,
                assignment_idx,
                ..
            }) = self.get_pending_var(idx)
            {
                return ExprNodeOperation::VariableSnapshot {
                    variable: idx,
                    assignment_id: *assignment_idx,
                    value: value.value().borrow().copy_as_ref(),
                }
                .to_expr();
            }
        }

        if let Some(value) = self.variables.get(&idx) {
            value.value.copy()
        } else {
            if let Some(alias) = self.predefined_aliases.get(&idx) {
                return self.get_var_with_allow_undefined(*alias, allow_undefined);
            }

            if allow_undefined {
                ExprNodeOperation::LocalVariable(idx).to_expr()
            } else {
                panic!("Variable {} not defined", idx)
            }
        }
    }
    pub fn get_var(&self, idx: usize) -> Expr {
        self.get_var_with_allow_undefined(idx, false)
    }

    fn next_assignment_id(&self) -> usize {
        let mut id = self.assignment_id_provider.borrow_mut();
        *id = id.wrapping_add(1).max(1);
        *id
    }

    fn run_assignment(&mut self, idx: usize, value: Expr) -> bool {
        let id = self.next_assignment_id();
        let is_new_variable = !self.variables.contains_key(&idx);
        self.variables.insert(
            idx,
            VariableValueSnapshot {
                value,
                defined_in_context: Some(self.context_id),
                assignment_idx: id,
            },
        );
        is_new_variable
    }

    pub fn push_branch_condition(&mut self, e: Expr) -> Result<(), anyhow::Error> {
        if self.last_branch_expr.is_some() {
            return Err(anyhow::anyhow!("Branch condition already pushed"));
        }
        self.last_branch_expr = Some(e);
        Ok(())
    }

    pub fn pop_branch_condition(&mut self) -> Option<Expr> {
        let expr = self.last_branch_expr.clone();
        self.last_branch_expr = None;
        expr
    }

    pub fn run(
        &mut self,
        bytecode: &Bytecode,
        dst_types: &Vec<Option<ReturnValueHint>>,
    ) -> Result<StacklessEvaluationRunResult, anyhow::Error> {
        if self.last_branch_expr.is_some() {
            return Err(anyhow::anyhow!(
                "Branch should be handled before running next bytecode"
            ));
        }
        let mut flushed_variables = HashSet::new();
        let mut new_variables = HashSet::new();
        match bytecode {
            Bytecode::Call(_, dsts, oper, srcs, _abort_action) => {
                for &dst in dsts {
                    if self.defined(dst) && self.get_var(dst).is_flushed() {
                        flushed_variables.insert(dst);
                    }
                }
                let allow_undefined = matches!(
                    oper,
                    move_stackless_bytecode::stackless_bytecode::Operation::Destroy
                );
                let OperationEvaluatorResult {
                    expr: results,
                    mut cannot_keep,
                } = oper.evaluate(
                    self,
                    &srcs
                        .iter()
                        .map(|x| {
                            if !self.defined(*x) {
                                ExprNodeOperation::Raw(format!("/*undefined:{}*/undefined", x))
                                    .to_expr()
                            } else {
                                self.get_var_with_allow_undefined(*x, allow_undefined)
                            }
                        })
                        .collect(),
                    dst_types,
                )?;

                let mut handled = false;
                match &results.value().borrow().operation {
                    ExprNodeOperation::ReadRef(..) => {}
                    ExprNodeOperation::WriteRef(_wdst, _wsrc) => {
                        if dsts.len() != 0 {
                            return Err(anyhow::anyhow!(
                                "Expected zero return value for write_ref"
                            ));
                        }
                        // FIXME: should we inc write for wdst?
                        handled = true;
                    }
                    ExprNodeOperation::DatatypeUnpack(_name, keys, _value, _types) => {
                        // special case - unpack to no variable
                        if dsts.len() == 0 {
                            handled = true;
                        } else {
                            if dsts.len() != keys.len() {
                                return Err(anyhow::anyhow!("Unmatched struct unpack"));
                            };
                            for dst in dsts {
                                if self.run_assignment(*dst, Expr::non_trivial()) {
                                    new_variables.insert(*dst);
                                }
                            }
                            handled = true;
                        }
                    }
                    _ => {}
                }

                if !handled {
                    if dsts.len() == 1 {
                        if self.run_assignment(dsts[0], results.copy()) {
                            new_variables.insert(dsts[0]);
                        }
                    } else {
                        for dst in dsts {
                            if self.run_assignment(*dst, Expr::non_trivial()) {
                                new_variables.insert(*dst);
                            }
                        }
                    }
                }

                if cannot_keep == false
                    && results
                        .collect_variables_with_count(false, true)
                        .any_variables()
                        .intersection(&HashSet::from_iter(dsts.iter().cloned()))
                        .next()
                        .is_some()
                {
                    cannot_keep = true;
                }

                Ok(StacklessEvaluationRunResult {
                    results,
                    new_variables,
                    flushed_variables,
                    cannot_keep_as_expr: cannot_keep,
                })
            }
            Bytecode::Assign(_, dst, src, kind) => {
                let dst = *dst;
                if self.defined(dst) && self.get_var(dst).is_flushed() {
                    flushed_variables.insert(dst);
                }
                let result = self.get_var(*src);
                if self.run_assignment(dst, result.copy()) {
                    new_variables.insert(dst);
                }
                match kind {
                    AssignKind::Copy => {}
                    AssignKind::Move => {
                        new_variables.insert(dst);
                        // value of src may be still referenced by other variables, the ownership already checked at compiler time so just ignore
                        // self.run_assignment(*src, Expr::deleted());
                    }
                    AssignKind::Store => {
                        // TODO: this is still a TODO in stackless bytecode too
                        // this assign is due to a COPY/MOVE pushed to the stack and poped
                        // it's seems that we dont need to do anything here
                    }
                };

                let cannot_keep = result
                    .collect_variables_with_count(false, true)
                    .any_variables()
                    .contains(&dst);

                Ok(StacklessEvaluationRunResult {
                    results: result,
                    new_variables,
                    flushed_variables,
                    cannot_keep_as_expr: cannot_keep,
                })
            }
            Bytecode::Load(_, dst, value) => {
                let dst = *dst;
                if self.defined(dst) && self.get_var(dst).is_flushed() {
                    flushed_variables.insert(dst);
                }
                let expr = ExprNodeOperation::Const(value.clone()).to_expr();
                if self.run_assignment(dst, expr.copy()) {
                    new_variables.insert(dst);
                }
                Ok(StacklessEvaluationRunResult {
                    results: expr,
                    new_variables,
                    flushed_variables,
                    cannot_keep_as_expr: false,
                })
            }
            Bytecode::Nop(..)
            | Bytecode::Ret(..)
            | Bytecode::Branch(..)
            | Bytecode::Jump(..)
            | Bytecode::Label(..)
            | Bytecode::Abort(..) => Ok(StacklessEvaluationRunResult {
                results: Expr::ignored(),
                new_variables,
                flushed_variables,
                cannot_keep_as_expr: false,
            }),
            Bytecode::VariantSwitch(..) => todo!(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn flush_value(&mut self, dst: usize, name: String, is_new: bool) {
        self.variables.insert(
            dst,
            VariableValueSnapshot {
                value: ExprNodeOperation::Raw(name.clone()).to_expr(),
                defined_in_context: if is_new { Some(self.context_id) } else { None },
                assignment_idx: usize::MAX,
            },
        );
    }

    pub(crate) fn flush_local_value(&mut self, dst: usize, is_new: Option<bool>) {
        let is_new = is_new.unwrap_or(!self.variables.contains_key(&dst));
        self.variables.insert(
            dst,
            VariableValueSnapshot {
                value: ExprNodeOperation::LocalVariable(dst).to_expr(),
                defined_in_context: if is_new { Some(self.context_id) } else { None },
                assignment_idx: usize::MAX,
            },
        );
    }

    fn is_flushed(&self, dst: usize) -> bool {
        if let Some(VariableValueSnapshot {
            assignment_idx: aid,
            ..
        }) = self.variables.get(&dst)
        {
            *aid == usize::MAX
        } else {
            false
        }
    }

    pub(crate) fn flush_pending_local_value(
        &mut self,
        dst: usize,
        is_new: Option<bool>,
        value: Expr,
    ) -> usize {
        if !self.defined(dst) {
            panic!("Invariant Exception: Variable {} not defined", dst);
        }
        if self.is_flushed(dst) {
            panic!("Invariant Exception: Variable {} already defined", dst);
        }
        let is_new = is_new.unwrap_or(!self.pending_variables.contains_key(&dst));
        let id = self.next_assignment_id();
        self.pending_variables.insert(
            dst,
            VariableValueSnapshot {
                value,
                defined_in_context: if is_new { Some(self.context_id) } else { None },
                assignment_idx: id,
            },
        );
        id
    }

    /// Assume that branches are starting from current context, merge them and return the variables that need to be flushed
    /// Currently not handling ignored and deleted variables, just consider these actions as an assignment
    pub(crate) fn merge_branches(
        &mut self,
        branches: &Vec<&StacklessEvaluationContext<'_>>,
        _self_not_in_tail: bool,
    ) -> Vec<usize> {
        let mut need_flushes = HashSet::new();

        for branch in branches {
            for (
                var_id,
                VariableValueSnapshot {
                    defined_in_context: br_context_definition_id,
                    assignment_idx: br_aid,
                    ..
                },
            ) in branch.variables.iter()
            {
                if let Some(VariableValueSnapshot {
                    assignment_idx: aid,
                    defined_in_context: current_context_definition_id,
                    ..
                }) = self.variables.get(var_id)
                {
                    if *aid != *br_aid
                        || *current_context_definition_id != *br_context_definition_id
                    {
                        need_flushes.insert(*var_id);
                    }
                } else {
                    need_flushes.insert(*var_id);
                }
            }
        }

        // for pending variables, we just ignore any that has conflict
        let mut pending_variables_to_remove = HashSet::new();
        for branch in branches {
            for (var_id, var_value) in branch.pending_variables.iter() {
                if let Some(self_var_value) = self.pending_variables.get(var_id) {
                    let conflict = var_value.assignment_idx != self_var_value.assignment_idx;
                    if conflict {
                        pending_variables_to_remove.insert(*var_id);
                    }
                }
            }
            self.finalized_pending_variables
                .extend(branch.finalized_pending_variables.iter());
        }

        for var_id in pending_variables_to_remove {
            self.pending_variables.remove(&var_id);
            self.finalized_pending_variables.insert(var_id);
        }

        need_flushes.into_iter().collect()
    }

    #[allow(dead_code)]
    pub(crate) fn get_vars(&self) -> HashSet<usize> {
        self.variables.keys().cloned().collect()
    }

    pub(crate) fn enter_loop(&mut self) {
        self.loop_entry = true;
    }

    pub(crate) fn add_pre_defined_alias(&mut self, from: &usize, to: &usize) {
        self.predefined_aliases.insert(*from, *to);
    }

    pub(crate) fn aliases(&self) -> &HashMap<usize, usize> {
        &self.predefined_aliases
    }
}
