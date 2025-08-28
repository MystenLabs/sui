// Copyright (c) Verichains, 2023

use std::collections::HashSet;

use move_stackless_bytecode::function_target::FunctionTarget;

use super::super::utils::{expr_and, expr_or};
use crate::decompiler::reconstruct::{
    ast::optimizers::utils::{
        blocks_iter_with_last_effective_indicator, has_effective_statement,
        last_effective_statements,
    },
    DecompiledCodeItem, DecompiledCodeUnit, DecompiledCodeUnitRef, DecompiledExpr,
};

/// if (cond) { expr1 } else { expr2 } -> cond && expr1 || expr2
/// let x; let y = if (x) { expr1 } else { expr2 } -> let y = x && expr1 || expr2
/// x must be read only once

pub(crate) fn rewrite_short_circuit_if_else(
    unit: &DecompiledCodeUnitRef,
    func_target: &FunctionTarget<'_>,
    defined_variables: &HashSet<usize>,
) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
    let mut blocked_variable_elimination = HashSet::new();

    loop {
        let mut eliminated_variables = HashSet::new();
        let new_unit = rewrite_short_circuit_if_else_recursive(
            unit,
            func_target,
            &defined_variables,
            &mut eliminated_variables,
            &mut blocked_variable_elimination,
        )?;

        if eliminated_variables
            .intersection(&blocked_variable_elimination)
            .next()
            .is_none()
        {
            return Ok(new_unit);
        }
    }
}

fn rewrite_short_circuit_if_else_recursive(
    unit: &DecompiledCodeUnitRef,
    func_target: &FunctionTarget<'_>,
    defined_variables: &HashSet<usize>,
    eliminated_variables: &mut HashSet<usize>,
    blocked_variable_elimination: &mut HashSet<usize>,
) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
    let mut new_unit = DecompiledCodeUnit::new();

    let mut defined_variables = defined_variables.clone();

    for item in unit.blocks.iter() {
        match item {
            DecompiledCodeItem::WhileStatement { cond, body } => {
                let body = rewrite_short_circuit_if_else_recursive(
                    body,
                    func_target,
                    &defined_variables,
                    eliminated_variables,
                    blocked_variable_elimination,
                )?;

                cond.as_ref().map(|c| {
                    update_blocked_variables(
                        &Vec::new(),
                        c,
                        blocked_variable_elimination,
                        eliminated_variables,
                    );
                });

                new_unit.add(DecompiledCodeItem::WhileStatement {
                    cond: cond.clone(),
                    body,
                });
            }

            DecompiledCodeItem::IfElseStatement {
                cond,
                if_unit,
                else_unit,
                result_variables,
                use_as_result,
            } => {
                let new_cond =
                    last_effective_statements::<1>(&new_unit.blocks).and_then(|[(idx, item)]| {
                        if let DecompiledCodeItem::AssignStatement {
                            variable,
                            value,
                            is_decl: true,
                        } = item
                        {
                            if blocked_variable_elimination.contains(variable) {
                                None
                            } else if cond
                                .is_single_variable_expr()
                                .map_or(false, |v| v == *variable)
                            {
                                eliminated_variables.insert(*variable);
                                Some((idx, value.clone()))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

                let cond = if let Some((idx, new_cond)) = new_cond {
                    new_unit.blocks.drain(idx..);
                    new_cond
                } else {
                    cond.clone()
                };

                let if_unit = rewrite_short_circuit_if_else_recursive(
                    if_unit,
                    func_target,
                    &defined_variables,
                    eliminated_variables,
                    blocked_variable_elimination,
                )?;
                let else_unit = rewrite_short_circuit_if_else_recursive(
                    else_unit,
                    func_target,
                    &defined_variables,
                    eliminated_variables,
                    blocked_variable_elimination,
                )?;

                if result_variables.len() == 1
                    && func_target.get_local_type(result_variables[0]).is_bool()
                    && !has_effective_statement(&if_unit.blocks)
                    && !has_effective_statement(&else_unit.blocks)
                    && if_unit.exit.is_some()
                    && else_unit.exit.is_some()
                {
                    let new_cond = DecompiledExpr::EvaluationExpr(
                        expr_or(
                            expr_and(cond.to_expr()?, if_unit.exit.as_ref().unwrap().to_expr()?),
                            else_unit.exit.as_ref().unwrap().to_expr()?,
                        )
                        .borrow()
                        .operation
                        .to_expr(),
                    )
                    .boxed();
                    update_blocked_variables(
                        &Vec::new(),
                        &new_cond,
                        blocked_variable_elimination,
                        eliminated_variables,
                    );
                    new_unit.add(DecompiledCodeItem::AssignStatement {
                        variable: result_variables[0],
                        value: new_cond,
                        is_decl: !defined_variables.contains(&result_variables[0]),
                    });
                    defined_variables.insert(result_variables[0]);
                } else {
                    new_unit.add(DecompiledCodeItem::IfElseStatement {
                        cond: cond.clone(),
                        if_unit,
                        else_unit,
                        result_variables: result_variables.clone(),
                        use_as_result: use_as_result.clone(),
                    });
                    defined_variables.extend(result_variables.iter());
                    update_blocked_variables(
                        &Vec::new(),
                        &cond,
                        blocked_variable_elimination,
                        eliminated_variables,
                    );
                }
            }
            DecompiledCodeItem::PossibleAssignStatement {
                variable,
                is_decl,
                value,
                ..
            }
            | DecompiledCodeItem::AssignStatement {
                variable,
                is_decl,
                value,
                ..
            } => {
                if *is_decl {
                    defined_variables.insert(*variable);
                }
                new_unit.add(item.clone());
                update_blocked_variables(
                    &vec![*variable],
                    value,
                    blocked_variable_elimination,
                    eliminated_variables,
                );
            }
            DecompiledCodeItem::AssignTupleStatement {
                variables,
                is_decl,
                value,
                ..
            } => {
                if *is_decl {
                    defined_variables.extend(variables.iter());
                }
                new_unit.add(item.clone());
                update_blocked_variables(
                    variables,
                    value,
                    blocked_variable_elimination,
                    eliminated_variables,
                );
            }
            DecompiledCodeItem::PreDeclareStatement { variable } => {
                defined_variables.insert(*variable);
                new_unit.add(item.clone());
            }
            DecompiledCodeItem::AssignStructureStatement {
                variables, value, ..
            } => {
                defined_variables.extend(variables.iter().map(|(_, v)| *v));
                new_unit.add(item.clone());
                update_blocked_variables(
                    &variables.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
                    value,
                    blocked_variable_elimination,
                    eliminated_variables,
                );
            }
            DecompiledCodeItem::ReturnStatement(expr)
            | DecompiledCodeItem::AbortStatement(expr)
            | DecompiledCodeItem::Statement { expr } => {
                new_unit.add(item.clone());
                update_blocked_variables(
                    &Vec::new(),
                    expr,
                    blocked_variable_elimination,
                    eliminated_variables,
                );
            }
            DecompiledCodeItem::BreakStatement
            | DecompiledCodeItem::ContinueStatement
            | DecompiledCodeItem::CommentStatement(_) => {
                new_unit.add(item.clone());
            }
        }
    }

    let effective_blocks: Vec<_> = blocks_iter_with_last_effective_indicator(&new_unit.blocks)
        .enumerate()
        .filter(|(_, item)| item.is_effective)
        .map(|(idx, _)| idx)
        .collect();

    if effective_blocks.len() == 1 {
        if let Some(v) = &unit.exit.as_ref().and_then(|x| x.is_single_variable_expr()) {
            let reduced_value = if let DecompiledCodeItem::AssignStatement {
                variable,
                value,
                is_decl: true,
            } = &new_unit.blocks[effective_blocks[0]]
            {
                if blocked_variable_elimination.contains(variable) {
                    None
                } else if variable == v {
                    Some(value.clone())
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(reduced_value) = reduced_value {
                eliminated_variables.insert(*v);
                new_unit.blocks.drain(effective_blocks[0]..);
                new_unit.exit = Some(reduced_value);
                return Ok(new_unit);
            }
        }
    }

    new_unit.exit = unit.exit.clone();
    new_unit.result_variables = unit.result_variables.clone();

    Ok(new_unit)
}

fn update_blocked_variables(
    variables: &[usize],
    c: &Box<DecompiledExpr>,
    blocked_variable_elimination: &mut HashSet<usize>,
    eliminated_variables: &mut HashSet<usize>,
) {
    let mut result_variables = HashSet::from_iter(variables.iter().cloned());
    let mut implicit_variables = HashSet::new();
    c.collect_variables(&mut result_variables, &mut implicit_variables, false, true);
    blocked_variable_elimination.extend(result_variables.intersection(&eliminated_variables));
}
