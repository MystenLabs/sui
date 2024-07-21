// Copyright (c) Verichains, 2023

use crate::decompiler::reconstruct::ast::optimizers::utils::BlockWithEffective;
use crate::decompiler::{
    evaluator::stackless::expr_node::ExprNodeOperation, reconstruct::ast::ResultUsageType,
};

use crate::decompiler::reconstruct::{
    expr::DecompiledExpr, DecompiledCodeItem, DecompiledCodeUnit, DecompiledCodeUnitRef,
};

use super::super::utils::blocks_iter_with_last_effective_indicator;

/// if (cond) { body } else { abort!(expr) } -> assert!(cond, expr); body;
pub(crate) fn rewrite_assert(
    unit: &DecompiledCodeUnitRef,
) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
    let mut new_unit = DecompiledCodeUnit::new();
    let mut need_copy_exit = true;

    for BlockWithEffective {
        block: item,
        is_last_effective,
        ..
    } in blocks_iter_with_last_effective_indicator(&unit.blocks)
    {
        match item {
            DecompiledCodeItem::IfElseStatement {
                cond,
                if_unit,
                else_unit,
                use_as_result,
                result_variables,
            } => {
                let mut transformed = false;
                let else_unit_effective_blocks: Vec<_> =
                    blocks_iter_with_last_effective_indicator(&else_unit.blocks)
                        .enumerate()
                        .filter(|(_, block)| block.is_effective)
                        .map(|(idx, _)| idx)
                        .collect();

                if else_unit_effective_blocks.len() == 1
                    && (use_as_result == &ResultUsageType::None || is_last_effective)
                {
                    if let DecompiledCodeItem::AbortStatement(expr) =
                        &else_unit.blocks[else_unit_effective_blocks[0]]
                    {
                        let rewritten_if_unit = rewrite_assert(if_unit)?;

                        new_unit.add(DecompiledCodeItem::Statement {
                            expr: DecompiledExpr::EvaluationExpr(
                                ExprNodeOperation::Func(
                                    "assert!".to_string(),
                                    vec![cond.to_expr()?, expr.to_expr()?],
                                    vec![],
                                )
                                .to_expr(),
                            )
                            .boxed(),
                        });

                        new_unit.extends(rewritten_if_unit.clone())?;

                        if use_as_result == &ResultUsageType::BlockResult && is_last_effective {
                            need_copy_exit = false;
                        }

                        transformed = true;
                    }
                }

                if !transformed {
                    new_unit.add(DecompiledCodeItem::IfElseStatement {
                        cond: cond.clone(),
                        if_unit: rewrite_assert(if_unit)?,
                        else_unit: rewrite_assert(else_unit)?,
                        result_variables: result_variables.clone(),
                        use_as_result: use_as_result.clone(),
                    });
                }
            }

            DecompiledCodeItem::WhileStatement { cond, body } => {
                new_unit.add(DecompiledCodeItem::WhileStatement {
                    cond: cond.clone(),
                    body: rewrite_assert(body)?,
                });
            }

            _ => {
                new_unit.add(item.clone());
            }
        }
    }

    if need_copy_exit {
        new_unit.exit = unit.exit.clone();
    }

    new_unit.result_variables = unit.result_variables.clone();

    Ok(new_unit)
}
