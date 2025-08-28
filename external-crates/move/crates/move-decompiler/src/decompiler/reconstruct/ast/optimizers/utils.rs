// Copyright (c) Verichains, 2023

use std::collections::HashSet;

use move_stackless_bytecode::stackless_bytecode::Constant;

use crate::decompiler::evaluator::stackless::expr_node::{
    effective_operation, ExprNodeOperation, ExprNodeRef,
};

use super::super::{DecompiledCodeItem, DecompiledCodeUnit};

pub(crate) fn collect_referenced_variables(
    unit: &DecompiledCodeUnit,
    referenced_variables: &mut HashSet<usize>,
    implicit_referenced_variables: &mut HashSet<usize>,
) {
    unit.exit.as_ref().map(|x| {
        x.collect_variables(
            referenced_variables,
            implicit_referenced_variables,
            false,
            true,
        )
    });
    for item in unit.blocks.iter() {
        match item {
            DecompiledCodeItem::PossibleAssignStatement {
                assignment_id: _,
                variable,
                value,
                is_decl,
            } => {
                value.collect_variables(
                    referenced_variables,
                    implicit_referenced_variables,
                    true,
                    true,
                );
                if !is_decl {
                    implicit_referenced_variables.insert(*variable);
                }
            }
            DecompiledCodeItem::PreDeclareStatement { variable } => {
                referenced_variables.insert(*variable);
            }
            DecompiledCodeItem::AssignStatement {
                variable,
                value,
                is_decl,
            } => {
                value.collect_variables(
                    referenced_variables,
                    implicit_referenced_variables,
                    false,
                    true,
                );
                if !is_decl {
                    referenced_variables.insert(*variable);
                }
            }
            DecompiledCodeItem::AssignTupleStatement {
                variables,
                value,
                is_decl,
            } => {
                value.collect_variables(
                    referenced_variables,
                    implicit_referenced_variables,
                    false,
                    true,
                );
                if !is_decl {
                    referenced_variables.extend(variables.iter());
                }
            }
            DecompiledCodeItem::AssignStructureStatement { value, .. } => {
                value.collect_variables(
                    referenced_variables,
                    implicit_referenced_variables,
                    false,
                    true,
                );
            }
            DecompiledCodeItem::IfElseStatement {
                result_variables,
                if_unit,
                else_unit,
                cond,
                ..
            } => {
                collect_referenced_variables(
                    if_unit,
                    referenced_variables,
                    implicit_referenced_variables,
                );
                collect_referenced_variables(
                    else_unit,
                    referenced_variables,
                    implicit_referenced_variables,
                );
                cond.as_ref().collect_variables(
                    referenced_variables,
                    implicit_referenced_variables,
                    false,
                    true,
                );
                referenced_variables.extend(result_variables.iter());
            }
            DecompiledCodeItem::WhileStatement { body, cond } => {
                collect_referenced_variables(
                    body,
                    referenced_variables,
                    implicit_referenced_variables,
                );
                cond.as_ref().map(|x| {
                    x.collect_variables(
                        referenced_variables,
                        implicit_referenced_variables,
                        false,
                        true,
                    )
                });
            }
            DecompiledCodeItem::Statement { expr: e }
            | DecompiledCodeItem::ReturnStatement(e)
            | DecompiledCodeItem::AbortStatement(e) => {
                e.collect_variables(
                    referenced_variables,
                    implicit_referenced_variables,
                    false,
                    true,
                );
            }
            DecompiledCodeItem::BreakStatement
            | DecompiledCodeItem::ContinueStatement
            | DecompiledCodeItem::CommentStatement(_) => {}
        }
    }
}

pub(crate) fn collect_live_variables(
    unit: &DecompiledCodeUnit,
    live_variables: &mut HashSet<usize>,
    implicit_variables: &mut HashSet<usize>,
) {
    unit.exit
        .as_ref()
        .map(|x| x.collect_variables(live_variables, implicit_variables, false, true));
    unit.result_variables.iter().for_each(|x| {
        live_variables.insert(*x);
        ()
    });
    for item in unit.blocks.iter() {
        match item {
            DecompiledCodeItem::PreDeclareStatement { variable } => {
                live_variables.insert(*variable);
            }
            DecompiledCodeItem::AssignTupleStatement {
                variables, value, ..
            } => {
                live_variables.extend(variables.iter());
                value.collect_variables(live_variables, implicit_variables, false, true);
            }
            DecompiledCodeItem::AssignStructureStatement {
                variables, value, ..
            } => {
                live_variables.extend(variables.iter().map(|x| x.1));
                value.collect_variables(live_variables, implicit_variables, false, true);
            }
            DecompiledCodeItem::PossibleAssignStatement {
                variable, value, ..
            } => {
                implicit_variables.insert(*variable);
                value.collect_variables(live_variables, implicit_variables, true, true);
            }
            DecompiledCodeItem::AssignStatement {
                variable, value, ..
            } => {
                live_variables.insert(*variable);
                value.collect_variables(live_variables, implicit_variables, false, true);
            }
            DecompiledCodeItem::IfElseStatement {
                result_variables,
                cond,
                if_unit,
                else_unit,
                ..
            } => {
                live_variables.extend(result_variables.iter());
                cond.collect_variables(live_variables, implicit_variables, false, true);
                collect_live_variables(if_unit, live_variables, implicit_variables);
                collect_live_variables(else_unit, live_variables, implicit_variables);
            }
            DecompiledCodeItem::WhileStatement { body, cond } => {
                if let Some(cond) = cond {
                    cond.collect_variables(live_variables, implicit_variables, false, true);
                }
                collect_live_variables(body, live_variables, implicit_variables);
            }
            DecompiledCodeItem::ReturnStatement(e) | DecompiledCodeItem::AbortStatement(e) => {
                e.collect_variables(live_variables, implicit_variables, false, true);
            }
            DecompiledCodeItem::BreakStatement
            | DecompiledCodeItem::ContinueStatement
            | DecompiledCodeItem::CommentStatement(_) => {}
            DecompiledCodeItem::Statement { expr } => {
                expr.collect_variables(live_variables, implicit_variables, false, true);
            }
        }
    }
}

pub(crate) fn get_variable_declaration_order(
    unit: &DecompiledCodeUnit,
    result_variables: &mut Vec<usize>,
) {
    for item in unit.blocks.iter() {
        match item {
            DecompiledCodeItem::PreDeclareStatement { variable } => {
                result_variables.push(*variable);
            }
            DecompiledCodeItem::AssignTupleStatement {
                variables, is_decl, ..
            } => {
                if *is_decl {
                    result_variables.extend(variables.iter());
                }
            }
            DecompiledCodeItem::AssignStructureStatement { variables, .. } => {
                result_variables.extend(variables.iter().map(|x| x.1));
            }
            DecompiledCodeItem::PossibleAssignStatement { .. } => {}
            DecompiledCodeItem::AssignStatement {
                variable, is_decl, ..
            } => {
                if *is_decl {
                    result_variables.push(*variable);
                }
            }
            DecompiledCodeItem::IfElseStatement {
                result_variables: r,
                if_unit,
                else_unit,
                ..
            } => {
                result_variables.extend(r.iter());
                get_variable_declaration_order(if_unit, result_variables);
                get_variable_declaration_order(else_unit, result_variables);
            }
            DecompiledCodeItem::WhileStatement { body, .. } => {
                get_variable_declaration_order(body, result_variables);
            }
            DecompiledCodeItem::ReturnStatement(..) | DecompiledCodeItem::AbortStatement(..) => {}
            DecompiledCodeItem::BreakStatement
            | DecompiledCodeItem::ContinueStatement
            | DecompiledCodeItem::CommentStatement(_) => {}
            DecompiledCodeItem::Statement { .. } => {}
        }
    }
}

pub(crate) fn is_effective_code_item(item: &DecompiledCodeItem) -> bool {
    !matches!(
        item,
        DecompiledCodeItem::CommentStatement(..)
            | DecompiledCodeItem::PossibleAssignStatement { .. }
    )
}

pub(crate) struct BlockWithEffective<T> {
    pub(crate) block: T,
    pub(crate) is_effective: bool,
    pub(crate) is_last_effective: bool,
}
pub(crate) fn blocks_iter_with_last_effective_indicator(
    blocks: &Vec<DecompiledCodeItem>,
) -> impl Iterator<Item = BlockWithEffective<&DecompiledCodeItem>> {
    let last_effective_idx = last_effective_idx_or_max(blocks);

    blocks
        .iter()
        .enumerate()
        .map(move |(index, x)| BlockWithEffective {
            block: x,
            is_effective: is_effective_code_item(x),
            is_last_effective: index == last_effective_idx,
        })
}

#[allow(dead_code)]
pub(crate) fn blocks_iter_mut_with_last_effective_indicator(
    blocks: &mut Vec<DecompiledCodeItem>,
) -> impl Iterator<Item = BlockWithEffective<&mut DecompiledCodeItem>> {
    let last_effective_idx = last_effective_idx_or_max(blocks);

    blocks.iter_mut().enumerate().map(move |(index, x)| {
        let is_effective = is_effective_code_item(x);
        BlockWithEffective {
            block: x,
            is_effective,
            is_last_effective: index == last_effective_idx,
        }
    })
}

fn last_effective_idx_or_max(blocks: &Vec<DecompiledCodeItem>) -> usize {
    blocks
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, x)| {
            if is_effective_code_item(x) {
                Some(index)
            } else {
                None
            }
        })
        .unwrap_or(usize::MAX)
}

/// Return the last effective statements in original order
pub(crate) fn last_effective_statements<const SIZE: usize>(
    blocks: &Vec<DecompiledCodeItem>,
) -> Option<[(usize, &DecompiledCodeItem); SIZE]> {
    let mut result = vec![];
    if SIZE == 0 {
        return Some(result.try_into().unwrap());
    }
    for (index, x) in blocks.iter().enumerate().rev() {
        if is_effective_code_item(x) {
            result.push((index, x));
            if result.len() == SIZE {
                break;
            }
        }
    }
    if result.len() == SIZE {
        result.reverse();
        Some(result.try_into().unwrap())
    } else {
        None
    }
}

pub(crate) fn last_effective_statement_mut(
    blocks: &mut Vec<DecompiledCodeItem>,
) -> Option<(usize, &mut DecompiledCodeItem)> {
    blocks.iter_mut().enumerate().rev().find_map(|(index, x)| {
        if is_effective_code_item(x) {
            Some((index, x))
        } else {
            None
        }
    })
}

pub(crate) fn has_effective_statement(blocks: &Vec<DecompiledCodeItem>) -> bool {
    blocks.iter().any(|x| is_effective_code_item(x))
}

#[allow(dead_code)]
pub(crate) fn expr_not(expr: ExprNodeRef) -> ExprNodeRef {
    if let Some(v) = effective_operation(&[&expr], &mut |[expr]| match &expr.borrow().operation {
        ExprNodeOperation::Const(Constant::Bool(x)) => {
            let toggled_value = !x;
            Some(
                ExprNodeOperation::Const(Constant::Bool(toggled_value))
                    .to_expr()
                    .value_copied(),
            )
        }
        _ => None,
    }) {
        return v;
    }
    ExprNodeOperation::Unary("!".to_string(), expr)
        .to_expr()
        .value_copied()
}

pub(crate) fn expr_and(expr1: ExprNodeRef, expr2: ExprNodeRef) -> ExprNodeRef {
    if let Some(v) = effective_operation(&[&expr1, &expr2], &mut |&[expr1, expr2]| match (
        &expr1.borrow().operation,
        &expr2.borrow().operation,
    ) {
        (ExprNodeOperation::Const(Constant::Bool(true)), _) => Some(expr2.clone()),
        (_, ExprNodeOperation::Const(Constant::Bool(true))) => Some(expr1.clone()),
        (ExprNodeOperation::Const(Constant::Bool(false)), _) => Some(expr1.clone()),
        _ => None,
    }) {
        return v;
    }
    ExprNodeOperation::Binary("&&".to_string(), expr1, expr2)
        .to_expr()
        .value_copied()
}

pub(crate) fn expr_or(expr1: ExprNodeRef, expr2: ExprNodeRef) -> ExprNodeRef {
    if let Some(v) = effective_operation(&[&expr1, &expr2], &mut |&[expr1, expr2]| match (
        &expr1.borrow().operation,
        &expr2.borrow().operation,
    ) {
        (ExprNodeOperation::Const(Constant::Bool(true)), _) => Some(expr1.clone()),
        (ExprNodeOperation::Const(Constant::Bool(false)), _) => Some(expr2.clone()),
        (_, ExprNodeOperation::Const(Constant::Bool(false))) => Some(expr1.clone()),
        _ => None,
    }) {
        return v;
    }

    ExprNodeOperation::Binary("||".to_string(), expr1, expr2)
        .to_expr()
        .value_copied()
}
