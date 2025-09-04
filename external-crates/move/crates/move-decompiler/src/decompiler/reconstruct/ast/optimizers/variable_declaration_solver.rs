// Copyright (c) Verichains, 2023

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use crate::decompiler::reconstruct::ast::expr::DecompiledExprRef;

/// This optimizer tries to find the best set of variables to be declared
/// wrt to the cost specified by exprs and some cost evaluator.
///
/// Variable is identified just by a usize, and has corresponding DecompiledExprRef as its value.
///
/// There must be no variable declared more than once.
///
/// The problem model is a set of assignments, set of expressions. Initially all
/// assignments are unmarked, so all expressions will directly use the value of these variable.
/// When an assignment is marked, we'll consider that as declaring a variable, and all expressions
/// that use the variable will be updated to use the declared variable. So solution model is
/// a set of marked assignments.
///
/// The case that a variable declared in some branch and used in later parent branch is not
/// in consideration of this optimizer.
pub(crate) struct VariableDeclarationOptimizer {
    variables: HashMap<usize, DecompiledExprRef>,
    blacklisted_variables: HashSet<usize>,
    exprs: Vec<DecompiledExprRef>,
}

impl VariableDeclarationOptimizer {
    pub(crate) fn new() -> Self {
        Self {
            variables: HashMap::new(),
            blacklisted_variables: HashSet::new(),
            exprs: Vec::new(),
        }
    }

    pub(crate) fn add_variable(&mut self, variable: &usize, value: &DecompiledExprRef) {
        if self.blacklisted_variables.contains(variable) {
            return;
        }
        if self.variables.contains_key(variable) {
            self.blacklisted_variables.insert(*variable);
            self.variables.remove(variable);
            return;
        }
        self.variables.insert(*variable, value.copy_as_ref());
    }

    pub(crate) fn add_expr(&mut self, expr: &DecompiledExprRef) {
        self.exprs.push(expr.copy_as_ref());
    }

    pub(crate) fn cleanup_non_referenced_variables(&mut self) {
        let referenced_variables = self
            .exprs
            .iter()
            .flat_map(|expr| {
                let mut normal_variables = HashSet::new();
                let mut implicit_variables = HashSet::new();
                expr.collect_variables(&mut normal_variables, &mut implicit_variables, false, true);
                implicit_variables
            })
            .collect::<HashSet<_>>();
        self.variables
            .retain(|variable, _| referenced_variables.contains(variable));
    }

    fn calculate_cost<ExprCost>(
        &self,
        selected_variables: &HashSet<usize>,
        expr_cost_evaluator: &dyn Fn(&DecompiledExprRef) -> ExprCost,
    ) -> Vec<ExprCost> {
        let mut selected_variables_sorted = selected_variables.iter().cloned().collect::<Vec<_>>();
        selected_variables_sorted.sort();
        self.exprs
            .iter()
            .map(|expr| expr_cost_evaluator(&expr.commit_pending_variables(selected_variables)))
            .chain(selected_variables_sorted.iter().map(|variable| {
                expr_cost_evaluator(
                    &self
                        .variables
                        .get(variable)
                        .unwrap()
                        .commit_pending_variables(selected_variables),
                )
            }))
            .collect::<Vec<_>>()
    }

    /// Search over all possible variable declarations and return the best one (the one with least cost)
    fn naive_solve<ExprCost>(
        &self,
        expr_cost_evaluator: &dyn Fn(&DecompiledExprRef) -> ExprCost,
        solution_comparator: &dyn Fn(&Vec<ExprCost>, &Vec<ExprCost>) -> Ordering,
        selected_variables: &HashSet<usize>,
    ) -> HashSet<usize> {
        let mut best_solution_variables = selected_variables.clone();
        let mut best_solution = self.calculate_cost(&best_solution_variables, expr_cost_evaluator);

        fn dfs<ExprCost>(
            self_: &VariableDeclarationOptimizer,
            expr_cost_evaluator: &dyn Fn(&DecompiledExprRef) -> ExprCost,
            solution_comparator: &dyn Fn(&Vec<ExprCost>, &Vec<ExprCost>) -> Ordering,

            best_solution_variables: &mut HashSet<usize>,
            best_solution: &mut Vec<ExprCost>,

            current_selected_variables: &mut HashSet<usize>,
            variables: &Vec<usize>,
            current_variable_idx: usize,
        ) {
            if current_variable_idx == variables.len() {
                let current_solution =
                    self_.calculate_cost(current_selected_variables, expr_cost_evaluator);
                if solution_comparator(&current_solution, best_solution) == Ordering::Less {
                    *best_solution_variables = current_selected_variables.clone();
                    *best_solution = current_solution;
                }
                return;
            }

            dfs::<ExprCost>(
                self_,
                expr_cost_evaluator,
                solution_comparator,
                best_solution_variables,
                best_solution,
                current_selected_variables,
                variables,
                current_variable_idx + 1,
            );

            current_selected_variables.insert(variables[current_variable_idx]);
            dfs::<ExprCost>(
                self_,
                expr_cost_evaluator,
                solution_comparator,
                best_solution_variables,
                best_solution,
                current_selected_variables,
                variables,
                current_variable_idx + 1,
            );
            current_selected_variables.remove(&variables[current_variable_idx]);
        }

        let mut current_selected_variables = selected_variables.clone();

        let mut variables = self
            .variables
            .keys()
            .filter(|x| !selected_variables.contains(x))
            .cloned()
            .collect::<Vec<usize>>();

        variables.sort();

        dfs::<ExprCost>(
            self,
            expr_cost_evaluator,
            solution_comparator,
            &mut best_solution_variables,
            &mut best_solution,
            &mut current_selected_variables,
            &variables,
            0,
        );

        best_solution_variables
    }

    pub(crate) fn solve<ExprCost>(
        &self,
        expr_cost_evaluator: &dyn Fn(&DecompiledExprRef) -> ExprCost,
        solution_comparator: &dyn Fn(&Vec<ExprCost>, &Vec<ExprCost>) -> Ordering,
    ) -> HashSet<usize> {
        let nvars = self.variables.len();
        let mut selected_variables = HashSet::new();
        let mut current_cost = self.calculate_cost(&selected_variables, expr_cost_evaluator);

        // to avoid too many combinations, we first try to reduce the number of variables to optimize by
        // greedily keep selecting the variable that can reduce the cost most until the number of variables
        // to optimize is small enough
        while nvars - selected_variables.len() > 10 {
            let best_var_to_declare = self
                .variables
                .keys()
                .filter(|x: &&usize| !selected_variables.contains(*x))
                .map(|v| {
                    let mut selected_variables = selected_variables.clone();
                    selected_variables.insert(*v);
                    let cost = self.calculate_cost(&selected_variables, expr_cost_evaluator);
                    (v, cost)
                })
                .min_by(|(v1, cost1), (v2, cost2)| {
                    let ord = solution_comparator(cost1, cost2);
                    if ord == Ordering::Equal {
                        v1.cmp(v2)
                    } else {
                        ord
                    }
                });
            if best_var_to_declare.is_none() {
                return selected_variables;
            }
            let (best_var_to_declare, cost) = best_var_to_declare.unwrap();
            if solution_comparator(&cost, &current_cost) == Ordering::Less {
                selected_variables.insert(*best_var_to_declare);
                current_cost = cost;
            } else {
                return selected_variables;
            }
        }

        let result = self.naive_solve(
            expr_cost_evaluator,
            solution_comparator,
            &selected_variables,
        );

        result
    }
}
