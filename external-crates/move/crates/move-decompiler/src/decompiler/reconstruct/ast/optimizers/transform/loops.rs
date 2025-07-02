// Copyright (c) Verichains, 2023

use std::collections::HashSet;

use crate::decompiler::reconstruct::ast::ResultUsageType;

use crate::decompiler::reconstruct::{DecompiledCodeItem, DecompiledCodeUnit};

use super::super::utils::blocks_iter_with_last_effective_indicator;

/// Rewrite loop to while loop
///
///   loop {                | while(expr) {
///     let var = expr;     |   [body]
///     if (var) {          | }
///         [body]          |
///     } else {            |
///         break;          |
///     }                   |
///   }                     |
///
/// (only when var is not used in body).
///
/// Also rewrite when there is no var too
///
///   loop {                | while(expr) {
///     if (expr) {         |   [body]
///         [body]          | }
///     } else {            |
///         break;          |
///     }                   |
///   }                     |
///
/// The body maybe terminated and break moved to outside of if
///
///   loop {
///     if (expr) {
///         [terminated body]
///     }
///     break;
///   }
///
pub(crate) fn rewrite_loop(unit: &mut DecompiledCodeUnit) -> Result<(), anyhow::Error> {
    for item in unit.blocks.iter_mut() {
        match item {
            DecompiledCodeItem::WhileStatement { cond, body } => {
                rewrite_loop(body)?;
                if cond.is_some() {
                    continue;
                }

                let effective_body_blocks: Vec<_> =
                    blocks_iter_with_last_effective_indicator(&body.blocks)
                        .enumerate()
                        .filter(|(_, item)| item.is_effective)
                        .map(|(idx, _)| idx)
                        .collect();

                if effective_body_blocks.len() == 1 {
                    if let DecompiledCodeItem::IfElseStatement {
                        cond: if_cond,
                        if_unit,
                        else_unit,
                        result_variables,
                        use_as_result,
                    } = &body.blocks[effective_body_blocks[0]]
                    {
                        if !result_variables.is_empty() || use_as_result != &ResultUsageType::None {
                            continue;
                        }

                        if else_unit.blocks.len() != 1 {
                            continue;
                        }

                        if !matches!(&else_unit.blocks[0], DecompiledCodeItem::BreakStatement) {
                            continue;
                        }

                        *cond = Some(if_cond.clone());
                        *body = if_unit.clone();
                    }
                } else if effective_body_blocks.len() == 2 {
                    // assign var = expr; if (var) { [body] } else { break; }
                    if let (
                        DecompiledCodeItem::AssignStatement {
                            variable,
                            value,
                            is_decl: true,
                        },
                        DecompiledCodeItem::IfElseStatement {
                            cond: if_cond,
                            if_unit,
                            else_unit,
                            result_variables,
                            use_as_result,
                        },
                    ) = (
                        &body.blocks[effective_body_blocks[0]],
                        &body.blocks[effective_body_blocks[1]],
                    ) {
                        if !result_variables.is_empty() || use_as_result != &ResultUsageType::None {
                            continue;
                        }

                        if let Some(v) = if_cond.is_single_variable_expr() {
                            if v != *variable {
                                continue;
                            }

                            if else_unit.blocks.len() != 1 {
                                continue;
                            }

                            if !matches!(&else_unit.blocks[0], DecompiledCodeItem::BreakStatement) {
                                continue;
                            }

                            if if_unit.has_reference_to_any_variable(&HashSet::from([*variable])) {
                                continue;
                            }

                            *cond = Some(value.clone());
                            *body = if_unit.clone();
                        }
                    }
                }
            }
            DecompiledCodeItem::IfElseStatement {
                if_unit, else_unit, ..
            } => {
                rewrite_loop(if_unit)?;
                rewrite_loop(else_unit)?;
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
