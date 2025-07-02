// Copyright (c) Verichains, 2023

use std::collections::{HashMap, HashSet};

use crate::decompiler::evaluator::stackless::expr_node::{ExprNodeOperation, ExprNodeRef};

use super::super::super::naming::Naming;

use super::super::super::evaluator::stackless::expr_node::Expr;


#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DecompiledExpr {
    Undefined,
    EvaluationExpr(Expr),
    #[allow(dead_code)]
    Variable(usize),
    Tuple(Vec<DecompiledExprRef>),
}

pub(crate) type DecompiledExprRef = Box<DecompiledExpr>;

impl DecompiledExpr {
    pub fn boxed(self: Self) -> DecompiledExprRef {
        Box::new(self)
    }

    pub fn copy_as_ref(&self) -> DecompiledExprRef {
        match self {
            DecompiledExpr::Undefined => DecompiledExpr::Undefined.boxed(),

            DecompiledExpr::EvaluationExpr(expr) => {
                DecompiledExpr::EvaluationExpr(expr.copy()).boxed()
            }

            DecompiledExpr::Variable(var) => DecompiledExpr::Variable(*var).boxed(),

            DecompiledExpr::Tuple(exprs) => {
                DecompiledExpr::Tuple(exprs.iter().map(|e| e.copy_as_ref()).collect()).boxed()
            }
        }
    }

    pub fn commit_pending_variables(
        &self,
        selected_variables: &HashSet<usize>,
    ) -> DecompiledExprRef {
        match self {
            DecompiledExpr::Undefined => DecompiledExpr::Undefined.boxed(),

            DecompiledExpr::EvaluationExpr(expr) => {
                DecompiledExpr::EvaluationExpr(expr.commit_pending_variables(selected_variables))
                    .boxed()
            }

            DecompiledExpr::Variable(var) => DecompiledExpr::Variable(*var).boxed(),

            DecompiledExpr::Tuple(exprs) => DecompiledExpr::Tuple(
                exprs
                    .iter()
                    .map(|e| e.commit_pending_variables(selected_variables))
                    .collect(),
            )
            .boxed(),
        }
    }

    pub fn is_single_or_tuple_variable_expr(&self) -> Option<Vec<usize>> {
        match self {
            DecompiledExpr::Tuple(exprs) => {
                exprs.iter().map(|e| e.is_single_variable_expr()).collect()
            }

            DecompiledExpr::EvaluationExpr(e) => e.is_single_variable().map(|v| vec![v]),

            DecompiledExpr::Variable(var) => Some(vec![*var]),

            _ => None,
        }
    }

    pub fn is_single_variable_expr(&self) -> Option<usize> {
        let vars = self.is_single_or_tuple_variable_expr()?;

        if vars.len() == 1 {
            Some(vars[0])
        } else {
            None
        }
    }

    pub fn has_reference_to_any_variable(&self, variables: &HashSet<usize>) -> bool {
        match self {
            DecompiledExpr::Undefined => false,

            DecompiledExpr::EvaluationExpr(expr) => expr.has_reference_to_any_variable(variables),

            DecompiledExpr::Variable(var) => variables.contains(var),

            DecompiledExpr::Tuple(exprs) => exprs
                .iter()
                .any(|e| e.has_reference_to_any_variable(variables)),
        }
    }

    pub fn rename_variables(&mut self, renamed_variables: &HashMap<usize, usize>) {
        self.rename_variables_opt(renamed_variables, true);
    }

    pub fn rename_variables_opt(
        &mut self,
        renamed_variables: &HashMap<usize, usize>,
        check_map_full: bool,
    ) {
        match self {
            DecompiledExpr::Undefined => {}

            DecompiledExpr::EvaluationExpr(expr) => {
                expr.rename_variables(renamed_variables, check_map_full);
            }

            DecompiledExpr::Variable(var) => {
                *var = renamed_variables[var];
            }

            DecompiledExpr::Tuple(exprs) => {
                for expr in exprs {
                    expr.rename_variables_opt(renamed_variables, check_map_full);
                }
            }
        }
    }

    pub fn collect_variables_with_count(
        &self,
        result_variables: &mut HashMap<usize, usize>,
        implicit_variables: &mut HashMap<usize, usize>,
        in_implicit_expr: bool,
        collect_inside_implicit_expr: bool,
    ) {
        match &self {
            DecompiledExpr::Undefined => {}

            DecompiledExpr::EvaluationExpr(expr) => {
                let var_info = expr
                    .collect_variables_with_count(in_implicit_expr, collect_inside_implicit_expr);
                var_info.variables.iter().for_each(|(var, count)| {
                    result_variables
                        .entry(*var)
                        .and_modify(|v| *v += count)
                        .or_insert(*count);
                });
                var_info.implicit_variables.iter().for_each(|(var, count)| {
                    implicit_variables
                        .entry(*var)
                        .and_modify(|v| *v += count)
                        .or_insert(*count);
                });
            }

            DecompiledExpr::Variable(var) => {
                if in_implicit_expr {
                    implicit_variables
                        .entry(*var)
                        .and_modify(|v| *v += 1)
                        .or_insert(1);
                } else {
                    result_variables
                        .entry(*var)
                        .and_modify(|v| *v += 1)
                        .or_insert(1);
                }
            }

            DecompiledExpr::Tuple(exprs) => {
                exprs.iter().for_each(|expr| {
                    expr.collect_variables_with_count(
                        result_variables,
                        implicit_variables,
                        in_implicit_expr,
                        collect_inside_implicit_expr,
                    )
                });
            }
        }
    }

    pub fn collect_variables(
        &self,
        result_variables: &mut HashSet<usize>,
        implicit_variables: &mut HashSet<usize>,
        in_implicit_expr: bool,
        collect_inside_implicit_expr: bool,
    ) {
        let mut result_variables_hm = HashMap::new();
        let mut implicit_variables_hm = HashMap::new();
        self.collect_variables_with_count(
            &mut result_variables_hm,
            &mut implicit_variables_hm,
            in_implicit_expr,
            collect_inside_implicit_expr,
        );
        result_variables.extend(result_variables_hm.keys());
        implicit_variables.extend(implicit_variables_hm.keys());
    }

    pub fn is_empty_tuple(&self) -> bool {
        match self {
            DecompiledExpr::Tuple(exprs) => exprs.is_empty(),

            _ => false,
        }
    }

    pub fn to_expr(&self) -> Result<ExprNodeRef, anyhow::Error> {
        match self {
            DecompiledExpr::Undefined => {
                Ok(ExprNodeOperation::Raw("undefined".to_string()).to_node())
            }

            DecompiledExpr::EvaluationExpr(expr) => Ok(expr.value_copied()),

            DecompiledExpr::Variable(var) => Ok(ExprNodeOperation::LocalVariable(*var).to_node()),

            DecompiledExpr::Tuple(exprs) => {
                if exprs.len() == 1 {
                    exprs[0].to_expr()
                } else {
                    Err(anyhow::anyhow!("Cannot convert tuple to expr"))
                }
            }
        }
    }

    pub fn to_source_decl(
        &self,
        naming: &Naming,
        standalone: bool,
    ) -> Result<String, anyhow::Error> {
        match self {
            DecompiledExpr::EvaluationExpr(expr) => Ok(expr.to_source_decl(naming, standalone)?),

            _ => self.to_source(naming, standalone),
        }
    }

    pub fn to_source(&self, naming: &Naming, standalone: bool) -> Result<String, anyhow::Error> {
        match self {
            DecompiledExpr::Undefined => Ok("undefined".to_string()),

            DecompiledExpr::EvaluationExpr(expr) => Ok(expr.to_source(naming, standalone)?),

            DecompiledExpr::Variable(var) => Ok(naming.variable(*var)),

            DecompiledExpr::Tuple(exprs) => {
                if exprs.len() == 1 {
                    exprs[0].to_source(naming, standalone)
                } else {
                    Ok(format!(
                        "({})",
                        exprs
                            .iter()
                            .map(|e| e.to_source(naming, true))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(", ")
                    ))
                }
            }
        }
    }
}
