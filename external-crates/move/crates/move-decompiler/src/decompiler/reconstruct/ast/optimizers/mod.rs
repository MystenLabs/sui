// Copyright (c) Verichains, 2023

use std::collections::{HashMap, HashSet};

use move_stackless_bytecode::function_target::FunctionTarget;

use crate::decompiler::naming::Naming;

use self::{
    structs::*,
    transform::{
        assert::*, cleanup_tail_exit::*, if_else::*, let_return::*, loops::*, non_source_blocks::*,
        variables::*,
    },
    utils::*,
    variable_declaration::*,
};

use super::super::DecompiledCodeUnitRef;
mod structs;
mod transform;
mod utils;
mod variable_declaration;
mod variable_declaration_solver;

pub struct OptimizerSettings {
    pub disable_optimize_variables_declaration: bool,
}

impl Default for OptimizerSettings {
    fn default() -> Self {
        Self {
            disable_optimize_variables_declaration: false,
        }
    }
}

pub(crate) fn run(
    unit: &DecompiledCodeUnitRef,
    func_target: &FunctionTarget<'_>,
    naming: &Naming,
    settings: &OptimizerSettings,
    alias: &HashMap<usize, usize>,
) -> Result<(DecompiledCodeUnitRef, HashSet<usize>), anyhow::Error> {
    let mut unit = unit.clone();

    let mut variable_index = VariableRenamingIndexMap::identity(func_target.get_local_count());

    let mut defined_variables = HashSet::new();
    for i in 0..func_target.get_parameter_count() {
        defined_variables.insert(i);
    }
    let mut in_alias = HashSet::new();
    for (k, v) in alias.iter() {
        in_alias.insert(*v);
        in_alias.insert(*k);
    }
    let mut unit = process_variable_alias(&mut unit, alias, &in_alias, &defined_variables)?;

    cleanup_tail_exit(&mut unit)?;
    let mut unit = rewrite_short_circuit_if_else(&unit, func_target, &defined_variables)?;

    rewrite_loop(&mut unit)?;
    rewrite_let_var_return(&mut unit)?;
    let mut unit = rewrite_assert(&unit)?;
    rewrite_let_if_return(&mut unit)?;

    unit = declare_wrt_borrow_checker(&unit, &variable_index, func_target)?;

    if !settings.disable_optimize_variables_declaration {
        rename_variables_by_order(&mut unit, &mut variable_index, func_target);
        unit = optimize_variables_declaration(&unit, naming)?;
    }

    let mut unit = remove_non_source_blocks(&unit)?;

    rename_variables_by_order(&mut unit, &mut variable_index, func_target);

    let mut referenced_variables = HashSet::new();
    let mut implicit_referenced_variables = HashSet::new();
    collect_referenced_variables(
        &unit,
        &mut referenced_variables,
        &mut implicit_referenced_variables,
    );

    Ok((unit, referenced_variables))
}

fn rename_variables_by_order(
    unit: &mut DecompiledCodeUnitRef,
    variable_index: &mut VariableRenamingIndexMap,
    func_target: &FunctionTarget<'_>,
) {
    let mut live_variables = HashSet::new();
    for i in 0..func_target.get_parameter_count() {
        live_variables.insert(i);
    }
    let mut implicit_variables = HashSet::new();
    collect_live_variables(&unit, &mut live_variables, &mut implicit_variables);

    // there maybe some implicit variables that are in live_variables already, just remove them
    implicit_variables = implicit_variables
        .difference(&live_variables)
        .map(|x| *x)
        .collect();

    let live_variables = live_variables.into_iter().collect::<Vec<_>>();

    let mut variables_declaration_order = Vec::new();
    get_variable_declaration_order(unit, &mut variables_declaration_order);

    let mut renamed_variables = HashMap::new();
    for i in 0..func_target.get_parameter_count() {
        renamed_variables.insert(i, renamed_variables.len());
    }
    for v in variables_declaration_order {
        if !renamed_variables.contains_key(&v) {
            renamed_variables.insert(v, renamed_variables.len());
        }
    }

    for v in live_variables.iter() {
        if !renamed_variables.contains_key(v) {
            renamed_variables.insert(*v, renamed_variables.len());
        }
    }
    let mut implicit_variables = implicit_variables.into_iter().collect::<Vec<_>>();
    implicit_variables.sort();
    for v in implicit_variables.iter() {
        if !renamed_variables.contains_key(v) {
            renamed_variables.insert(*v, renamed_variables.len());
        }
    }
    rename_variables(unit, &renamed_variables);

    variable_index.apply(&renamed_variables);
}
