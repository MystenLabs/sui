// Copyright (c) Verichains, 2023

use move_stackless_bytecode::function_target;

use crate::decompiler::{
    naming::Naming,
    reconstruct::ast::expr::{DecompiledExpr, DecompiledExprRef},
};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::{cmp::Ordering, collections::HashSet};

use super::structs::VariableRenamingIndexMap;
use super::variable_declaration_solver::VariableDeclarationOptimizer;

use super::super::DecompiledCodeItem as I;
use super::super::DecompiledCodeUnitRef;

#[derive(Clone, Debug, Default)]
struct Violations {
    solution: HashSet<usize>,
}

#[derive(Clone, Debug, Default)]
struct PossibleVarRef {
    declaration_time: usize,
    inner_possible_vars: HashSet<usize>,
    direct_borrows: HashSet<usize>,
}

#[derive(Clone)]
struct BorrowCheckerState<'s> {
    reference_type_variables: &'s HashMap<usize, bool>,
    time_provider: Rc<RefCell<usize>>,
    violations: Rc<RefCell<Violations>>,

    borrows: HashMap<usize, HashSet<usize>>,
    violated_borrows: HashMap<usize, usize>,
    borrowed_by: HashMap<usize, HashSet<usize>>,
    borrowed_by_t: HashMap<(usize, usize), usize>,
    possible_var_ref: HashMap<usize, PossibleVarRef>,
}

impl<'s> BorrowCheckerState<'s> {
    fn new(ref_variables: &'s HashMap<usize, bool>) -> Self {
        Self {
            reference_type_variables: ref_variables,
            time_provider: Rc::new(RefCell::new(0)),
            borrows: HashMap::new(),
            borrowed_by: HashMap::new(),
            borrowed_by_t: HashMap::new(),
            violations: Rc::new(RefCell::new(Violations::default())),
            possible_var_ref: HashMap::new(),
            violated_borrows: HashMap::new(),
        }
    }

    fn next_time(&self) -> usize {
        let mut time = self.time_provider.borrow_mut();
        *time += 1;
        *time
    }

    fn fork(&self) -> Self {
        self.clone()
    }

    fn filter_references<'i, T>(&self, vars: T) -> Vec<usize>
    where
        T: IntoIterator<Item = &'i usize>,
    {
        let mut result = Vec::new();
        for &var in vars {
            if self.reference_type_variables.contains_key(&var) {
                result.push(var);
            }
        }
        result
    }

    fn commit_references(&mut self, vars: &Vec<usize>) {
        for var in vars.iter() {
            for borrow in self.borrows.remove(var).unwrap_or_default() {
                if let Some(borrowed_by) = self.borrowed_by.get_mut(&borrow) {
                    borrowed_by.remove(var);
                    self.borrowed_by_t.remove(&(borrow, *var));
                }
            }
            self.violated_borrows.remove(var);
        }
    }

    fn assign(&mut self, vars: &Vec<usize>, value: &DecompiledExpr, _is_decl: bool) {
        self.commit_references(vars);

        let (_referenced_variables, _implicit_variables, value_borrows) =
            self.rvalue(value, true, true);
        if value_borrows.is_empty() {
            return;
        }

        let t = self.next_time();

        let ref_vars: Vec<_> = vars
            .iter()
            .filter(|x| self.reference_type_variables.contains_key(x))
            .collect();
        if ref_vars.is_empty() {
            return;
        }

        let mut borrows = HashSet::new();
        for &borrowed in value_borrows.iter() {
            borrows.insert(borrowed);
        }

        for &&var in ref_vars.iter() {
            self.borrows.insert(var, borrows.clone());
        }

        for &borrow in borrows.iter() {
            self.borrowed_by
                .entry(borrow)
                .or_insert_with(Default::default)
                .extend(ref_vars.iter().cloned());
            for &&var in ref_vars.iter() {
                self.borrowed_by_t.insert((borrow, var), t);
            }
        }
    }

    fn possible_assign(&mut self, variable: usize, value: &DecompiledExpr, is_decl: bool) {
        debug_assert!(
            is_decl,
            "this module assume all possible-assign are declarations"
        );

        let t = self.next_time();

        let (referenced_variables, implicit_variables, _) = self.rvalue(value, false, false);

        let direct_borrows: HashSet<_> = self
            .filter_references(referenced_variables.iter())
            .iter()
            .cloned()
            .collect();

        let vr = self
            .possible_var_ref
            .entry(variable)
            .or_insert_with(Default::default);

        vr.declaration_time = t;
        vr.inner_possible_vars = implicit_variables;
        vr.direct_borrows = direct_borrows;
    }

    fn rvalue(
        &mut self,
        expr: &DecompiledExpr,
        set_violation: bool,
        check_violation: bool,
    ) -> (
        /*referenced*/ HashSet<usize>,
        /*implicit*/ HashSet<usize>,
        /*borrows*/ HashSet<usize>,
    ) {
        let t = self.next_time();

        let mut referenced_variables = HashSet::new();
        let mut implicit_variables = HashSet::new();

        expr.collect_variables(
            &mut referenced_variables,
            &mut implicit_variables,
            false,
            false,
        );

        let mut borrows = self.resolve_implicit_variables_borrows(&implicit_variables);

        borrows.extend(self.filter_references(referenced_variables.iter()).iter());

        if check_violation {
            let violations: HashMap<_, _> = borrows
                .iter()
                .map(|var| self.violated_borrows.get(var).map(|t| (*var, *t)))
                .flatten()
                .collect();
            if !violations.is_empty() {
                self.heuristic_solve_violation(&violations, &implicit_variables);
            }
        }

        if set_violation {
            for var in borrows.iter() {
                if let Some(br) = self.borrowed_by.get(var) {
                    for b in br.iter() {
                        if !self.violated_borrows.contains_key(b) {
                            self.violated_borrows.insert(*b, t);
                        }
                    }
                }
            }
        }

        (referenced_variables, implicit_variables, borrows)
    }

    fn resolve_implicit_variables_borrows(
        &self,
        implicit_variables: &HashSet<usize>,
    ) -> HashSet<usize> {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        for &var in implicit_variables.iter() {
            queue.push_back(var);
            visited.insert(var);
        }
        let mut result = HashSet::new();
        while let Some(from_var) = queue.pop_front() {
            if let Some(var) = self.possible_var_ref.get(&from_var) {
                for &var in var.inner_possible_vars.iter() {
                    if visited.contains(&var) {
                        continue;
                    }
                    queue.push_back(var);
                }
                result.extend(var.direct_borrows.iter().cloned());
            }
        }
        result
    }

    fn expand_implicit_variables(&self, implicit_variables: &HashSet<usize>) -> Vec<usize> {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        for &var in implicit_variables.iter() {
            queue.push_back(var);
            visited.insert(var);
        }
        let mut result = Vec::new();
        while let Some(from_var) = queue.pop_front() {
            result.push(from_var);
            if let Some(var) = self.possible_var_ref.get(&from_var) {
                for &var in var.inner_possible_vars.iter() {
                    if !visited.contains(&var) {
                        queue.push_back(var);
                    }
                }
            }
        }
        result
    }

    fn heuristic_solve_violation(
        &mut self,
        violations: &HashMap<usize, usize>,
        implicit_variables: &HashSet<usize>,
    ) {
        if violations.is_empty() {
            return;
        }
        let mut implicit_variables = self.expand_implicit_variables(implicit_variables);
        implicit_variables.sort_by_key(|var| {
            match self.possible_var_ref.get(var).map(|x| x.declaration_time) {
                Some(t) => usize::MAX - t,
                None => usize::MAX,
            }
        });
        let mut remain_violations = violations.clone();
        for implicit_var in implicit_variables.iter() {
            let Some(implicit_var_t) = self
                .possible_var_ref
                .get(implicit_var)
                .map(|x| x.declaration_time)
            else {
                continue;
            };
            let borrows =
                self.resolve_implicit_variables_borrows(&HashSet::from_iter(vec![*implicit_var]));
            // check if this implicit variable violate any violation
            if borrows.iter().any(|b| {
                violations
                    .get(b)
                    .map(|t| *t <= implicit_var_t)
                    .unwrap_or(false)
            }) {
                continue;
            };

            // check if this implicit variable can solve any violation
            let can_solve: Vec<_> = remain_violations
                .iter()
                .map(|(b, _)| b)
                .filter(|b| borrows.contains(b))
                .cloned()
                .collect();

            if can_solve.is_empty() {
                continue;
            }

            self.violations.borrow_mut().solution.insert(*implicit_var);
            for b in can_solve.iter() {
                remain_violations.remove(b);
            }

            if remain_violations.is_empty() {
                return;
            }
        }
    }
}

// solve borrows checker in decompiled code by pushing borrows to before any violation
pub(crate) fn declare_wrt_borrow_checker(
    unit: &DecompiledCodeUnitRef,
    variable_index: &VariableRenamingIndexMap,
    function_target: &function_target::FunctionTarget<'_>,
) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
    let mut ref_variables = HashMap::new();
    for &i in variable_index.current_variables().iter() {
        let original = variable_index.get(i);
        let typ = function_target.get_local_type(original);
        if typ.is_reference() {
            ref_variables.insert(i, typ.is_mutable_reference());
        }
    }
    let mut state = BorrowCheckerState::new(&ref_variables);

    collect_borrow_checker_violations(unit, variable_index, function_target, &mut state)?;

    let solution = state.violations.borrow().solution.clone();

    if solution.is_empty() {
        return Ok(unit.clone());
    }

    apply_variable_declaration(unit, &solution, false)
}

// return if the unit is terminated
fn collect_borrow_checker_violations(
    unit: &DecompiledCodeUnitRef,
    variable_index: &VariableRenamingIndexMap,
    function_target: &function_target::FunctionTarget<'_>,
    state: &mut BorrowCheckerState,
) -> Result<bool, anyhow::Error> {
    for item in unit.blocks.iter() {
        match item {
            I::IfElseStatement {
                if_unit,
                else_unit,
                cond,
                ..
            } => {
                state.rvalue(cond, false, true);
                let mut if_state = state.fork();
                let if_terminated = collect_borrow_checker_violations(
                    if_unit,
                    variable_index,
                    function_target,
                    &mut if_state,
                )?;
                let mut else_state = state.fork();
                let else_terminated = collect_borrow_checker_violations(
                    else_unit,
                    variable_index,
                    function_target,
                    &mut else_state,
                )?;
                if if_terminated && else_terminated {
                    return Ok(true);
                }
            }
            I::WhileStatement { body, cond } => {
                cond.as_ref().map(|c| state.rvalue(c, false, true));
                let mut body_state = state.fork();
                let body_terminated = collect_borrow_checker_violations(
                    body,
                    variable_index,
                    function_target,
                    &mut body_state,
                )?;
                if body_terminated {
                    return Ok(true);
                }
            }
            I::AssignStatement {
                variable,
                value,
                is_decl,
            } => {
                state.assign(&vec![*variable], value, *is_decl);
            }
            I::AssignTupleStatement {
                variables,
                value,
                is_decl,
            } => {
                state.assign(variables, value, *is_decl);
            }
            I::AssignStructureStatement {
                variables, value, ..
            } => {
                state.assign(&variables.iter().map(|x| x.1).collect(), value, true);
            }
            I::PossibleAssignStatement {
                assignment_id: _,
                value,
                variable,
                is_decl,
            } => {
                state.possible_assign(*variable, value, *is_decl);
            }
            I::ReturnStatement(expr) | I::AbortStatement(expr) => {
                state.rvalue(expr, false, true);
                return Ok(true);
            }
            I::Statement { expr, .. } => {
                state.rvalue(expr, false, true);
            }
            I::BreakStatement
            | I::ContinueStatement
            | I::CommentStatement(_)
            | I::PreDeclareStatement { .. } => {}
        }
    }

    Ok(false)
}

pub(crate) fn optimize_variables_declaration(
    unit: &DecompiledCodeUnitRef,
    naming: &Naming,
) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
    #[derive(Debug)]
    struct ExprCost {
        source_len: usize,
    }

    let expr_cost = |expr: &DecompiledExprRef| -> ExprCost {
        let source = expr.to_source(naming, false).unwrap();

        ExprCost {
            source_len: source.len(),
        }
    };

    // heuristic - less is better
    fn cost_compare(a: &Vec<ExprCost>, b: &Vec<ExprCost>) -> Ordering {
        const LINE_LENGTH: usize = 100;

        let max_source_len_a = a.iter().map(|x| x.source_len).max().unwrap_or(0);
        let max_source_len_b = b.iter().map(|x| x.source_len).max().unwrap_or(0);

        let a_source_len_overflow = max_source_len_a > LINE_LENGTH;
        let b_source_len_overflow = max_source_len_b > LINE_LENGTH;

        if a_source_len_overflow != b_source_len_overflow {
            return if a_source_len_overflow {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }

        if a_source_len_overflow {
            let ord = max_source_len_a.cmp(&max_source_len_b);
            if ord != Ordering::Equal {
                return ord;
            }
        }

        let ord = a.len().cmp(&b.len());
        if ord != Ordering::Equal {
            return ord;
        }

        if a_source_len_overflow {
            let ord = max_source_len_b.cmp(&max_source_len_a);
            if ord != Ordering::Equal {
                return ord;
            }
        }
        for (a, b) in a.iter().zip(b.iter()) {
            let ord = a.source_len.cmp(&b.source_len);
            if ord != Ordering::Equal {
                return ord;
            }
        }

        Ordering::Equal
    }

    let mut solver: VariableDeclarationOptimizer = VariableDeclarationOptimizer::new();
    initialize_solver(&mut solver, unit);
    solver.cleanup_non_referenced_variables();
    let should_declare = solver.solve(&expr_cost, &cost_compare);
    apply_variable_declaration(unit, &should_declare, true)
}

fn initialize_solver(solver: &mut VariableDeclarationOptimizer, unit: &DecompiledCodeUnitRef) {
    for item in unit.blocks.iter() {
        match item {
            I::IfElseStatement {
                if_unit,
                else_unit,
                cond,
                ..
            } => {
                solver.add_expr(cond);
                initialize_solver(solver, if_unit);
                initialize_solver(solver, else_unit);
            }
            I::WhileStatement { body, cond } => {
                if let Some(cond) = cond {
                    solver.add_expr(cond);
                }
                initialize_solver(solver, body);
            }
            I::ReturnStatement(expr)
            | I::AbortStatement(expr)
            | I::AssignStatement { value: expr, .. }
            | I::AssignTupleStatement { value: expr, .. }
            | I::AssignStructureStatement { value: expr, .. }
            | I::Statement { expr, .. } => {
                solver.add_expr(expr);
            }
            I::BreakStatement
            | I::ContinueStatement
            | I::CommentStatement(_)
            | I::PreDeclareStatement { .. } => {}
            I::PossibleAssignStatement {
                assignment_id: _,
                value,
                variable,
                is_decl,
            } => {
                if *is_decl {
                    solver.add_variable(variable, value);
                }
            }
        }
    }
}
fn apply_variable_declaration(
    unit: &DecompiledCodeUnitRef,
    should_declare: &HashSet<usize>,
    remove_possible_assign: bool,
) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
    let mut new_unit: Box<crate::decompiler::reconstruct::ast::DecompiledCodeUnit> = unit.clone();
    new_unit.blocks.clear();
    for item in unit.blocks.iter() {
        match item {
            I::IfElseStatement {
                if_unit,
                else_unit,
                cond,
                result_variables,
                use_as_result,
            } => {
                let new_if_unit =
                    apply_variable_declaration(if_unit, should_declare, remove_possible_assign)?;
                let new_else_unit =
                    apply_variable_declaration(else_unit, should_declare, remove_possible_assign)?;
                new_unit.blocks.push(I::IfElseStatement {
                    if_unit: new_if_unit,
                    else_unit: new_else_unit,
                    cond: cond.commit_pending_variables(should_declare),
                    result_variables: result_variables.clone(),
                    use_as_result: use_as_result.clone(),
                });
            }
            I::WhileStatement { body, cond } => {
                let new_body =
                    apply_variable_declaration(body, should_declare, remove_possible_assign)?;
                new_unit.blocks.push(I::WhileStatement {
                    body: new_body,
                    cond: cond
                        .clone()
                        .map(|c| c.commit_pending_variables(should_declare)),
                });
            }
            I::ReturnStatement(expr) => {
                new_unit.blocks.push(I::ReturnStatement(
                    expr.commit_pending_variables(should_declare),
                ));
            }
            I::AbortStatement(expr) => {
                new_unit.blocks.push(I::AbortStatement(
                    expr.commit_pending_variables(should_declare),
                ));
            }
            I::AssignStatement {
                variable,
                value,
                is_decl,
            } => {
                new_unit.blocks.push(I::AssignStatement {
                    variable: *variable,
                    value: value.commit_pending_variables(should_declare),
                    is_decl: *is_decl,
                });
            }
            I::AssignTupleStatement {
                variables,
                value,
                is_decl,
            } => {
                new_unit.blocks.push(I::AssignTupleStatement {
                    variables: variables.clone(),
                    value: value.commit_pending_variables(should_declare),
                    is_decl: *is_decl,
                });
            }
            I::AssignStructureStatement {
                structure_visible_name,
                variables,
                value,
            } => {
                new_unit.blocks.push(I::AssignStructureStatement {
                    structure_visible_name: structure_visible_name.clone(),
                    variables: variables.clone(),
                    value: value.commit_pending_variables(should_declare),
                });
            }
            I::Statement { expr } => {
                new_unit.blocks.push(I::Statement {
                    expr: expr.commit_pending_variables(should_declare),
                });
            }
            I::BreakStatement | I::ContinueStatement | I::CommentStatement(_) => {
                new_unit.blocks.push(item.clone());
            }
            I::PreDeclareStatement { variable } => {
                new_unit.blocks.push(I::PreDeclareStatement {
                    variable: *variable,
                });
            }
            I::PossibleAssignStatement {
                assignment_id: _,
                value,
                variable,
                is_decl,
            } => {
                if *is_decl && should_declare.contains(variable) {
                    new_unit.blocks.push(I::AssignStatement {
                        variable: *variable,
                        value: value.commit_pending_variables(should_declare),
                        is_decl: true,
                    });
                } else if !remove_possible_assign {
                    new_unit.blocks.push(I::PossibleAssignStatement {
                        assignment_id: *variable,
                        value: value.commit_pending_variables(should_declare),
                        variable: *variable,
                        is_decl: *is_decl,
                    });
                }
            }
        }
    }
    new_unit.exit = new_unit
        .exit
        .map(|x| x.commit_pending_variables(should_declare));
    Ok(new_unit)
}
