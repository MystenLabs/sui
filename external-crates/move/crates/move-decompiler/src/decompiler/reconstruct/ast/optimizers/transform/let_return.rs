// Copyright (c) Verichains, 2023

use crate::decompiler::reconstruct::ast::ResultUsageType;

use crate::decompiler::reconstruct::{
    expr::DecompiledExpr, DecompiledCodeItem, DecompiledCodeUnit,
};

use super::super::utils::{last_effective_statement_mut, last_effective_statements};

/// let var = expr; return var; -> return expr;
pub(crate) fn rewrite_let_var_return(unit: &mut DecompiledCodeUnit) -> Result<(), anyhow::Error> {
    if let Some((let_idx, exit)) = check_let_return(&unit.blocks) {
        unit.blocks.drain(let_idx..);
        unit.add(DecompiledCodeItem::ReturnStatement(exit));
    } else if let Some((let_idx, exit)) = check_let_exit(&unit.blocks, &unit.exit) {
        unit.blocks.drain(let_idx..);
        unit.exit = Some(exit);
    }

    update_let_if_exit(&mut unit.blocks, &mut unit.exit);

    for item in unit.blocks.iter_mut() {
        match item {
            DecompiledCodeItem::IfElseStatement {
                if_unit, else_unit, ..
            } => {
                rewrite_let_var_return(if_unit)?;
                rewrite_let_var_return(else_unit)?;
            }

            DecompiledCodeItem::WhileStatement { body, .. } => {
                rewrite_let_var_return(body)?;
            }

            DecompiledCodeItem::ReturnStatement(_)
            | DecompiledCodeItem::AbortStatement(_)
            | DecompiledCodeItem::BreakStatement
            | DecompiledCodeItem::ContinueStatement
            | DecompiledCodeItem::CommentStatement(_)
            | DecompiledCodeItem::PossibleAssignStatement { .. }
            | DecompiledCodeItem::PreDeclareStatement { .. }
            | DecompiledCodeItem::AssignStatement { .. }
            | DecompiledCodeItem::AssignTupleStatement { .. }
            | DecompiledCodeItem::AssignStructureStatement { .. }
            | DecompiledCodeItem::Statement { .. } => {}
        }
    }

    Ok(())
}

fn check_let_return(blocks: &Vec<DecompiledCodeItem>) -> Option<(usize, Box<DecompiledExpr>)> {
    if let Some([(aidx, a), (_, b)]) = last_effective_statements::<2>(blocks) {
        if let (
            DecompiledCodeItem::AssignStatement {
                variable,
                value,
                is_decl: true,
            },
            DecompiledCodeItem::ReturnStatement(expr),
        ) = (a, b)
        {
            if expr
                .is_single_variable_expr()
                .map(|x| x == *variable)
                .unwrap_or(false)
            {
                // as the block is returned, current_exit can be ignored
                return Some((aidx, value.clone()));
            }
        }

        if let (
            DecompiledCodeItem::AssignTupleStatement {
                variables,
                value,
                is_decl: true,
            },
            DecompiledCodeItem::ReturnStatement(expr),
        ) = (a, b)
        {
            if expr
                .is_single_or_tuple_variable_expr()
                .map(|x| &x == variables)
                .unwrap_or(false)
            {
                // as the block is returned, current_exit can be ignored
                return Some((aidx, value.clone()));
            }
        }
    }

    None
}

fn check_let_exit(
    blocks: &Vec<DecompiledCodeItem>,
    current_exit: &Option<Box<DecompiledExpr>>,
) -> Option<(usize, Box<DecompiledExpr>)> {
    if current_exit.is_none() {
        return None;
    }

    if let Some([(aidx, a)]) = last_effective_statements::<1>(blocks) {
        if let DecompiledCodeItem::AssignStatement {
            variable,
            value,
            is_decl: true,
        } = a
        {
            if current_exit
                .as_ref()
                .unwrap()
                .is_single_variable_expr()
                .map(|x| &x == variable)
                .unwrap_or(false)
            {
                return Some((aidx, value.clone()));
            }
        }

        if let DecompiledCodeItem::AssignTupleStatement {
            variables,
            value,
            is_decl: true,
        } = a
        {
            if variables.len() > 0
                && current_exit
                    .as_ref()
                    .unwrap()
                    .is_single_or_tuple_variable_expr()
                    .map(|x| &x == variables)
                    .unwrap_or(false)
            {
                return Some((aidx, value.clone()));
            }
        }
    }

    None
}

fn update_let_if_exit(
    blocks: &mut Vec<DecompiledCodeItem>,
    current_exit: &mut Option<Box<DecompiledExpr>>,
) {
    if let Some((idx, stmt)) = last_effective_statement_mut(blocks) {
        if let DecompiledCodeItem::IfElseStatement {
            result_variables,
            use_as_result,
            ..
        } = stmt
        {
            match use_as_result {
                ResultUsageType::None => {
                    if result_variables.len() == 0 {
                        return;
                    }
                }
                ResultUsageType::BlockResult => {
                    debug_assert!(result_variables.len() == 0);
                }
                _ => {}
            }

            if current_exit
                .as_ref()
                .map(|x| {
                    x.is_single_or_tuple_variable_expr()
                        .map(|x| &x == result_variables)
                        .unwrap_or(false)
                })
                .unwrap_or(false)
            {
                *current_exit = None;
                result_variables.clear();
                *use_as_result = ResultUsageType::BlockResult;
                blocks.drain(idx + 1..);
            }
        }
    }
}

/// let var = if_expr; return var; -> return if_expr;
pub(crate) fn rewrite_let_if_return(unit: &mut DecompiledCodeUnit) -> Result<(), anyhow::Error> {
    if let Some((if_index, return_type)) = check_let_if_return(&unit.blocks) {
        // drop all non-source blocks after the if statement and the return statement itself
        unit.blocks.drain((if_index + 1)..);
        // the last statement is now if statement

        if let DecompiledCodeItem::IfElseStatement { use_as_result, .. } =
            unit.blocks.last_mut().unwrap()
        {
            *use_as_result = return_type;
        } else {
            unreachable!();
        }
    }

    for item in unit.blocks.iter_mut() {
        match item {
            DecompiledCodeItem::IfElseStatement {
                if_unit, else_unit, ..
            } => {
                rewrite_let_if_return(if_unit)?;
                rewrite_let_if_return(else_unit)?;
            }

            DecompiledCodeItem::WhileStatement { body, .. } => {
                rewrite_let_if_return(body)?;
            }

            DecompiledCodeItem::ReturnStatement(_)
            | DecompiledCodeItem::AbortStatement(_)
            | DecompiledCodeItem::BreakStatement
            | DecompiledCodeItem::ContinueStatement
            | DecompiledCodeItem::CommentStatement(_)
            | DecompiledCodeItem::PossibleAssignStatement { .. }
            | DecompiledCodeItem::PreDeclareStatement { .. }
            | DecompiledCodeItem::AssignStatement { .. }
            | DecompiledCodeItem::AssignTupleStatement { .. }
            | DecompiledCodeItem::AssignStructureStatement { .. }
            | DecompiledCodeItem::Statement { .. } => {}
        }
    }

    Ok(())
}

fn check_let_if_return(blocks: &Vec<DecompiledCodeItem>) -> Option<(usize, ResultUsageType)> {
    if let Some([(if_index, a), (_, b)]) = last_effective_statements::<2>(blocks) {
        if let (
            DecompiledCodeItem::IfElseStatement {
                result_variables,
                use_as_result: ResultUsageType::None,
                ..
            },
            DecompiledCodeItem::ReturnStatement(expr) | DecompiledCodeItem::AbortStatement(expr),
        ) = (a, b)
        {
            if result_variables.len() > 0
                && expr
                    .is_single_or_tuple_variable_expr()
                    .map(|x| &x == result_variables)
                    .unwrap_or(false)
            {
                return match blocks[blocks.len() - 1] {
                    DecompiledCodeItem::ReturnStatement(_) => {
                        Some((if_index, ResultUsageType::Return))
                    }

                    DecompiledCodeItem::AbortStatement(_) => {
                        Some((if_index, ResultUsageType::Abort))
                    }

                    _ => unreachable!(),
                };
            }
        }
    }

    None
}
