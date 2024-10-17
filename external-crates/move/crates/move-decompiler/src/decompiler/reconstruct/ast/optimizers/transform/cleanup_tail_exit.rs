// Copyright (c) Verichains, 2023

use super::super::utils::last_effective_statement_mut;
use crate::decompiler::reconstruct::{
    ast::ResultUsageType, DecompiledCodeItem, DecompiledCodeUnitRef,
};

/// Cleanup tail exit
///  {...; let vars = if_expr; vars} -> {...; if_expr}

pub(crate) fn cleanup_tail_exit(unit: &mut DecompiledCodeUnitRef) -> Result<(), anyhow::Error> {
    for item in unit.blocks.iter_mut() {
        match item {
            DecompiledCodeItem::IfElseStatement {
                if_unit, else_unit, ..
            } => {
                cleanup_tail_exit(if_unit)?;
                cleanup_tail_exit(else_unit)?;
            }

            DecompiledCodeItem::WhileStatement { body, .. } => {
                cleanup_tail_exit(body)?;
            }

            _ => {}
        }
    }

    if let Some((
        idx,
        DecompiledCodeItem::IfElseStatement {
            result_variables,
            use_as_result,
            ..
        },
    )) = last_effective_statement_mut(&mut unit.blocks) {
        if result_variables.len() > 0 && (result_variables == &unit.result_variables) {
            unit.exit = None;
            result_variables.clear();
            *use_as_result = ResultUsageType::BlockResult;
            unit.blocks.drain(idx + 1..);
        }
    }

    Ok(())
}
