// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::model::move_model::{Field, Module, Package, Struct};
/// A set of simple walkers for the Move model.
/// Those are not strictly needed and somewhat questionable, we'll see in time if they are useful.
/// All the walker take a `closure` and get called for each element they are walking.
/// The idea is to use the `GlobalEnv` and to use the functions in this module without having
/// to know the details of the model.
use crate::model::{
    global_env::GlobalEnv,
    move_model::{Bytecode, Function},
};

/// Walk each bytecode in the environment.
/// The closure receives each bytecode and the function it belongs to.
pub fn walk_bytecodes<F>(env: &GlobalEnv, mut walker: F)
where
    F: FnMut(&GlobalEnv, &Function, &Bytecode),
{
    env.functions.iter().for_each(|func| {
        if let Some(code) = func.code.as_ref() {
            code.code
                .iter()
                .for_each(|bytecode| walker(env, func, bytecode));
        }
    });
}

/// Walk all fields in the environment.
/// The closure receives each field and the struct it belongs to.
pub fn walk_fields<F>(env: &GlobalEnv, mut walker: F)
where
    F: FnMut(&GlobalEnv, &Struct, &Field),
{
    env.structs.iter().for_each(|struct_| {
        struct_
            .fields
            .iter()
            .for_each(|field| walker(env, struct_, field));
    });
}

/// Walk all packages in the environment.
pub fn walk_packages<F>(env: &GlobalEnv, mut walker: F)
where
    F: FnMut(&GlobalEnv, &Package),
{
    env.packages.iter().for_each(|package| walker(env, package));
}

/// Walk all modules in the environment.
pub fn walk_modules<F>(env: &GlobalEnv, mut walker: F)
where
    F: FnMut(&GlobalEnv, &Module),
{
    env.modules.iter().for_each(|module| walker(env, module));
}

/// Walk all functions in the environment.
pub fn walk_functions<F>(env: &GlobalEnv, mut walker: F)
where
    F: FnMut(&GlobalEnv, &Function),
{
    env.functions
        .iter()
        .for_each(|function| walker(env, function));
}

/// Walk all structs in the environment.
pub fn walk_structs<F>(env: &GlobalEnv, mut walker: F)
where
    F: FnMut(&GlobalEnv, &Struct),
{
    env.structs.iter().for_each(|struct_| walker(env, struct_));
}
