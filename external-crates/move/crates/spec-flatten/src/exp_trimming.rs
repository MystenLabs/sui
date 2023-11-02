// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::collections::BTreeMap;

use move_model::{
    ast::{ConditionKind, Spec},
    model::{FunId, QualifiedId},
};
use move_stackless_bytecode::function_target::FunctionTarget;

use crate::workflow::{prepare_with_override, prove, WorkflowOptions};

pub(crate) fn trim_aborts_ifs(
    options: &WorkflowOptions,
    target: FunctionTarget,
    spec: Spec,
) -> Result<Spec> {
    let fun_env = target.func_env;
    let mut fun_options = options.clone();
    fun_options.target = Some(fun_env.get_simple_name_string().to_string());

    let new_spec =
        remove_redundant_aborts_ifs_since(&fun_options, fun_env.get_qualified_id(), spec, 0)?;
    Ok(new_spec)
}

fn remove_first_aborts_if_since(spec: &Spec, pos: usize) -> (Spec, bool) {
    let Spec {
        loc,
        conditions,
        properties,
        on_impl,
    } = spec.clone();

    let mut new_conditions = vec![];
    let mut changed = false;
    for (i, cond) in conditions.into_iter().enumerate() {
        if changed || i < pos || !matches!(cond.kind, ConditionKind::AbortsIf) {
            new_conditions.push(cond);
        } else {
            changed = true;
        }
    }

    let new_spec = Spec {
        loc,
        conditions: new_conditions,
        properties,
        on_impl,
    };
    (new_spec, changed)
}

fn remove_first_aborts_if_and_prove_since(
    options: &WorkflowOptions,
    fun_id: QualifiedId<FunId>,
    spec: &Spec,
    pos: usize,
) -> Result<Option<(Spec, bool)>> {
    let (new_spec, changed) = remove_first_aborts_if_since(spec, pos);
    if !changed {
        return Ok(None);
    }

    let mut spec_override = BTreeMap::new();
    spec_override.insert(fun_id, new_spec.clone());
    let (env, targets) = prepare_with_override(options, spec_override)?;
    let proved = prove(options, &env, &targets)?;
    Ok(Some((new_spec, proved)))
}

fn remove_redundant_aborts_ifs_since(
    options: &WorkflowOptions,
    fun_id: QualifiedId<FunId>,
    spec: Spec,
    pos: usize,
) -> Result<Spec> {
    match remove_first_aborts_if_and_prove_since(options, fun_id, &spec, pos)? {
        None => {
            // no more aborts_if conditions to remove
            Ok(spec)
        }
        Some((new_spec, true)) => {
            // removing one aborts_if does not affect the proving
            remove_redundant_aborts_ifs_since(options, fun_id, new_spec, pos)
        }
        Some((_, false)) => {
            // removing one aborts_if makes the proving failed
            remove_redundant_aborts_ifs_since(options, fun_id, spec, pos + 1)
        }
    }
}
