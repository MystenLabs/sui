// Copyright (c) Verichains, 2023

use crate::decompiler::reconstruct::{DecompiledCodeItem, DecompiledCodeUnitRef};

/// Remove non source blocks
pub(crate) fn remove_non_source_blocks(
    unit: &DecompiledCodeUnitRef,
) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
    let mut new_blocks = Vec::new();

    for item in unit.blocks.iter() {
        match item {
            DecompiledCodeItem::PossibleAssignStatement { .. } => {}

            DecompiledCodeItem::IfElseStatement {
                cond,
                if_unit,
                else_unit,
                result_variables,
                use_as_result,
            } => {
                let if_unit = remove_non_source_blocks(&if_unit)?;
                let else_unit = remove_non_source_blocks(&else_unit)?;
                new_blocks.push(DecompiledCodeItem::IfElseStatement {
                    cond: cond.clone(),
                    if_unit,
                    else_unit,
                    result_variables: result_variables.clone(),
                    use_as_result: use_as_result.clone(),
                });
            }

            DecompiledCodeItem::WhileStatement { body, cond } => {
                let body = remove_non_source_blocks(&body)?;
                new_blocks.push(DecompiledCodeItem::WhileStatement {
                    cond: cond.clone(),
                    body,
                });
            }

            _ => {
                new_blocks.push(item.clone());
            }
        }
    }

    // the removals wont affect exit or result_variables, so we just need
    // to set the new blocks
    let mut unit = unit.clone();
    unit.blocks = new_blocks;

    Ok(unit)
}
