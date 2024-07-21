// Copyright (c) Verichains, 2023

use std::collections::{HashMap, HashSet};

use crate::decompiler::reconstruct::{
    ast::DecompiledCodeUnitRef, DecompiledCodeItem, DecompiledCodeUnit,
};

pub(crate) fn rename_variables(
    unit: &mut DecompiledCodeUnit,
    renamed_variables: &HashMap<usize, usize>,
) {
    unit.exit
        .as_mut()
        .map(|x| x.rename_variables(renamed_variables));

    unit.result_variables = unit
        .result_variables
        .iter()
        .map(|x| renamed_variables[x])
        .collect();

    for item in unit.blocks.iter_mut() {
        match item {
            DecompiledCodeItem::AbortStatement(x) | DecompiledCodeItem::ReturnStatement(x) => {
                x.rename_variables(renamed_variables)
            }

            DecompiledCodeItem::PreDeclareStatement { variable } => {
                *variable = renamed_variables[variable];
            }

            DecompiledCodeItem::AssignStatement {
                variable, value, ..
            } => {
                *variable = renamed_variables[variable];
                value.rename_variables(renamed_variables);
            }

            DecompiledCodeItem::PossibleAssignStatement {
                variable, value, ..
            } => {
                *variable = renamed_variables[variable];
                value.rename_variables(renamed_variables);
            }

            DecompiledCodeItem::AssignTupleStatement {
                variables, value, ..
            } => {
                for v in variables.iter_mut() {
                    *v = renamed_variables[v];
                }
                value.rename_variables(renamed_variables);
            }

            DecompiledCodeItem::BreakStatement
            | DecompiledCodeItem::ContinueStatement
            | DecompiledCodeItem::CommentStatement(_) => {}
            DecompiledCodeItem::AssignStructureStatement {
                variables, value, ..
            } => {
                for v in variables.iter_mut() {
                    v.1 = renamed_variables[&v.1];
                }
                value.rename_variables(renamed_variables);
            }

            DecompiledCodeItem::Statement { expr } => {
                expr.rename_variables(renamed_variables);
            }

            DecompiledCodeItem::IfElseStatement {
                cond,
                if_unit,
                else_unit,
                result_variables,
                ..
            } => {
                cond.rename_variables(renamed_variables);
                for v in result_variables.iter_mut() {
                    *v = renamed_variables[v];
                }
                rename_variables(if_unit, renamed_variables);
                rename_variables(else_unit, renamed_variables);
            }

            DecompiledCodeItem::WhileStatement { cond, body } => {
                cond.as_mut().map(|x| x.rename_variables(renamed_variables));
                rename_variables(body, renamed_variables);
            }
        }
    }
}

pub(crate) fn process_variable_alias(
    unit: &DecompiledCodeUnitRef,
    alias: &HashMap<usize, usize>,
    in_alias: &HashSet<usize>,
    defined: &HashSet<usize>,
) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
    let unit = unit.as_ref();

    let mut defined = defined.clone();

    let mut new_unit = DecompiledCodeUnit::new();

    new_unit.exit = unit.exit.as_ref().map(|x| {
        let mut cloned = x.clone();
        cloned.rename_variables_opt(alias, false);
        cloned
    });

    new_unit.result_variables = unit
        .result_variables
        .iter()
        .map(|x| alias.get(x).unwrap_or(x).clone())
        .collect();

    for item in unit.blocks.iter() {
        match item {
            DecompiledCodeItem::AbortStatement(x) => {
                let mut x = x.copy_as_ref();
                x.rename_variables_opt(alias, false);
                new_unit.blocks.push(DecompiledCodeItem::AbortStatement(x));
            }

            DecompiledCodeItem::ReturnStatement(x) => {
                let mut x = x.copy_as_ref();
                x.rename_variables_opt(alias, false);
                new_unit.blocks.push(DecompiledCodeItem::ReturnStatement(x));
            }

            DecompiledCodeItem::PreDeclareStatement { variable } => {
                if defined.contains(variable) {
                    if !in_alias.contains(variable) {
                        return Err(anyhow::anyhow!("Invariant: variable definition conflict: multiple definition of variable"));
                    }
                    continue;
                }
                if alias.contains_key(variable) {
                    let aliased_variable = alias.get(variable).unwrap().clone();
                    if defined.contains(&aliased_variable) {
                        continue;
                    }
                    new_unit
                        .blocks
                        .push(DecompiledCodeItem::PreDeclareStatement {
                            variable: aliased_variable,
                        });
                    defined.insert(aliased_variable);
                } else {
                    new_unit.blocks.push(item.clone());
                    defined.insert(*variable);
                }
            }

            DecompiledCodeItem::AssignStatement {
                variable,
                value,
                is_decl,
            } => {
                let variable = alias.get(variable).unwrap_or(variable).clone();
                let mut value = value.copy_as_ref();
                value.rename_variables_opt(alias, false);
                if value
                    .is_single_variable_expr()
                    .map(|v| alias.get(&v).unwrap_or(&v) == &variable)
                    .unwrap_or(false)
                {
                    continue;
                }
                let new_is_decl = !defined.contains(&variable);
                if (*is_decl != new_is_decl && !new_is_decl) && !in_alias.contains(&variable) {
                    return Err(anyhow::anyhow!("Invariant: variable definition conflict: declaration not match for assignment, variable {} declaration is defined as {} but calculated as {}", variable, *is_decl, new_is_decl));
                }
                let is_decl = new_is_decl;
                new_unit.blocks.push(DecompiledCodeItem::AssignStatement {
                    variable,
                    value,
                    is_decl,
                });
                if is_decl {
                    defined.insert(variable);
                }
            }

            DecompiledCodeItem::PossibleAssignStatement {
                variable,
                value,
                assignment_id,
                is_decl,
            } => {
                let variable = alias.get(variable).unwrap_or(variable).clone();
                let mut value = value.copy_as_ref();
                value.rename_variables_opt(alias, false);
                if value
                    .is_single_variable_expr()
                    .map(|v| alias.get(&v).unwrap_or(&v) == &variable)
                    .unwrap_or(false)
                {
                    continue;
                }
                let new_is_decl = !defined.contains(&variable);
                if (*is_decl != new_is_decl && !new_is_decl) && !in_alias.contains(&variable) {
                    return Err(anyhow::anyhow!("Invariant: variable definition conflict: declaration not match for possible assignment, variable {} declaration is defined as {} but calculated as {}", variable, *is_decl, new_is_decl));
                }
                let is_decl = new_is_decl;
                if !is_decl {
                    new_unit.blocks.push(DecompiledCodeItem::AssignStatement {
                        variable,
                        value,
                        is_decl,
                    });
                } else {
                    new_unit
                        .blocks
                        .push(DecompiledCodeItem::PossibleAssignStatement {
                            variable,
                            value,
                            assignment_id: *assignment_id,
                            is_decl,
                        });
                }
            }

            DecompiledCodeItem::AssignTupleStatement {
                variables,
                value,
                is_decl,
            } => {
                let variables: Vec<_> = variables
                    .iter()
                    .map(|x| alias.get(x).unwrap_or(x).clone())
                    .collect();
                let mut value = value.copy_as_ref();
                value.rename_variables_opt(alias, false);
                if *is_decl {
                    for v in variables.iter() {
                        defined.insert(*v);
                    }
                }
                new_unit
                    .blocks
                    .push(DecompiledCodeItem::AssignTupleStatement {
                        variables,
                        value,
                        is_decl: *is_decl,
                    });
            }

            DecompiledCodeItem::BreakStatement
            | DecompiledCodeItem::ContinueStatement
            | DecompiledCodeItem::CommentStatement(_) => {
                new_unit.blocks.push(item.clone());
            }
            DecompiledCodeItem::AssignStructureStatement {
                variables,
                value,
                structure_visible_name,
            } => {
                let variables: Vec<_> = variables
                    .iter()
                    .map(|x| (x.0.clone(), alias.get(&x.1).unwrap_or(&x.1).clone()))
                    .collect();
                let mut value = value.copy_as_ref();
                value.rename_variables_opt(alias, false);
                for v in variables.iter() {
                    defined.insert(v.1);
                }
                new_unit
                    .blocks
                    .push(DecompiledCodeItem::AssignStructureStatement {
                        variables,
                        value,
                        structure_visible_name: structure_visible_name.clone(),
                    });
            }

            DecompiledCodeItem::Statement { expr } => {
                let mut expr = expr.copy_as_ref();
                expr.rename_variables_opt(alias, false);
                new_unit.blocks.push(DecompiledCodeItem::Statement { expr });
            }

            DecompiledCodeItem::IfElseStatement {
                cond,
                if_unit,
                else_unit,
                result_variables,
                use_as_result,
            } => {
                let mut cond = cond.copy_as_ref();
                cond.rename_variables_opt(alias, false);
                let result_variables: Vec<_> = result_variables
                    .iter()
                    .map(|x| alias.get(x).unwrap_or(x).clone())
                    .collect();
                let if_unit = process_variable_alias(if_unit, alias, in_alias, &defined)?;
                let else_unit = process_variable_alias(else_unit, alias, in_alias, &defined)?;
                for v in result_variables.iter() {
                    defined.insert(*v);
                }
                new_unit.blocks.push(DecompiledCodeItem::IfElseStatement {
                    cond,
                    if_unit,
                    else_unit,
                    result_variables,
                    use_as_result: use_as_result.clone(),
                });
            }

            DecompiledCodeItem::WhileStatement { cond, body } => {
                let mut cond = cond.clone();
                cond.as_mut().map(|x| x.rename_variables_opt(alias, false));
                let body = process_variable_alias(body, alias, in_alias, &defined)?;
                new_unit
                    .blocks
                    .push(DecompiledCodeItem::WhileStatement { cond, body });
            }
        }
    }

    Ok(new_unit)
}
