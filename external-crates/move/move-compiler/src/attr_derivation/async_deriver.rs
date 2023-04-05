// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    attr_derivation::{
        find_attr_slice, new_borrow_exp, new_call_exp, new_full_name, new_fun, new_simple_name_exp,
        new_simple_type, new_struct, new_u64, new_var,
    },
    diag,
    parser::ast::{
        Definition, Exp, Exp_, Field, Function, FunctionName, LeadingNameAccess_, ModuleDefinition,
        ModuleMember, NameAccessChain_, StructFields, StructName, Type, Type_, Visibility,
    },
    shared::{CompilationEnv, NamedAddressMap},
};
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::{sp, Loc};
use move_symbol_pool::Symbol;
use sha3::{Digest, Sha3_256};
use std::convert::TryInto;

const ACTOR_ATTR: &str = "actor";
const STATE_ATTR: &str = "state";
const INIT_ATTR: &str = "init";
const MESSAGE_ATTR: &str = "message";
const MAX_SEND_PARAM_COUNT: usize = 8;

pub(crate) fn derive_for_async(
    env: &mut CompilationEnv,
    address_map: &NamedAddressMap,
    def: &mut Definition,
) {
    if let Definition::Module(mod_def) = def {
        derive_module_for_async(env, address_map, mod_def)
    }
}

fn derive_module_for_async(
    env: &mut CompilationEnv,
    address_map: &NamedAddressMap,
    mod_def: &mut ModuleDefinition,
) {
    if find_attr_slice(&mod_def.attributes, ACTOR_ATTR).is_none() {
        // Not an actor module
        return;
    }

    let state_name = check_state(env, mod_def);

    // Go over the functions marked as #[message]
    let mut new_funs = vec![];
    let mut new_structs = vec![];
    for mem in &mod_def.members {
        if let ModuleMember::Function(fun_def) = mem {
            if let Some(attr) = find_attr_slice(&fun_def.attributes, MESSAGE_ATTR) {
                // Generate `send_<name>(actor: address, message_hash: vector<u8>, <args>)`.
                let loc = attr.loc;
                let call_name =
                    FunctionName(sp(loc, Symbol::from(format!("send_{}", fun_def.name))));

                // Replace the first parameter which is a reference to the actor state with an
                // actor address.
                let mut sign = fun_def.signature.clone();
                let mut fields = vec![];
                if sign.parameters.is_empty() {
                    env.add_diag(diag!(
                        Derivation::DeriveFailed,
                        (
                            loc,
                            "expected at least one parameter for message handler".to_string()
                        )
                    ));
                    continue;
                }
                check_state_ref_param(env, &sign.parameters[0].1, &state_name);
                sign.parameters[0] = (
                    new_var(loc, "actor"),
                    new_simple_type(loc, "address", vec![]),
                );
                fields.push((
                    Field(sp(loc, Symbol::from("actor"))),
                    new_simple_type(loc, "address", vec![]),
                ));

                // Create a call `Actor::send__<N>(actor, message_hash, BCS::to_bytes(&arg1), ..)`.
                let param_count = sign.parameters.len() - 1;
                if param_count > MAX_SEND_PARAM_COUNT {
                    env.add_diag(diag!(
                        Derivation::DeriveFailed,
                        (
                            loc,
                            format!(
                                "too many arguments for message handler (current max is {})",
                                MAX_SEND_PARAM_COUNT
                            )
                        )
                    ))
                }
                let mut args = vec![new_simple_name_exp(loc, sp(loc, Symbol::from("actor")))];
                args.push(message_hash(env, address_map, loc, mod_def, fun_def));
                fields.push((
                    Field(sp(loc, Symbol::from("message_hash"))),
                    new_simple_type(loc, "u64", vec![]),
                ));

                for i in 0..param_count {
                    let name_exp = new_simple_name_exp(loc, sign.parameters[i + 1].0 .0);
                    args.push(new_call_exp(
                        loc,
                        new_full_name(loc, "std", "bcs", "to_bytes"),
                        vec![new_borrow_exp(loc, name_exp)],
                    ));
                    let var = &sign.parameters[i + 1].0;
                    let ty = &sign.parameters[i + 1].1;
                    fields.push((Field(sp(loc, var.0.value)), ty.clone()));
                }
                let internal_send_name = format!("send__{}", param_count);
                let send_call = new_call_exp(
                    loc,
                    new_full_name(loc, "Async", "Runtime", &internal_send_name),
                    args,
                );

                new_funs.push(new_fun(
                    loc,
                    call_name,
                    sp(loc, vec![]),
                    Visibility::Public(loc),
                    None,
                    sign,
                    send_call,
                ));

                let mut struct_name = fun_def.name.to_string();
                struct_name.replace_range(0..1, &struct_name[0..1].to_uppercase());
                new_structs.push(new_struct(
                    loc,
                    StructName(sp(loc, Symbol::from(struct_name))),
                    StructFields::Defined(fields),
                ));
            }
        }
    }
    for fun_def in new_funs {
        mod_def.members.push(ModuleMember::Function(fun_def))
    }
    for st_def in new_structs {
        mod_def.members.push(ModuleMember::Struct(st_def))
    }
}

fn check_state(env: &mut CompilationEnv, mod_def: &ModuleDefinition) -> Option<StructName> {
    // Find the associated state of the actor.
    let state_structs = mod_def
        .members
        .iter()
        .filter_map(|m| match m {
            ModuleMember::Struct(sdef)
                if find_attr_slice(&sdef.attributes, STATE_ATTR).is_some() =>
            {
                Some(sdef)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let state_name = if state_structs.is_empty() {
        env.add_diag(diag!(
            Derivation::DeriveFailed,
            (
                mod_def.loc,
                "actor module must have a struct marked as #[state]".to_string()
            )
        ));
        None
    } else {
        if state_structs.len() > 1 {
            env.add_diag(diag!(
                Derivation::DeriveFailed,
                (
                    state_structs[1].loc,
                    "actor module must have only one struct marked as #[state]".to_string()
                )
            ));
        }
        Some(state_structs[0].name)
    };

    // Check whether the init function is present and correctly typed.
    let init_funs = mod_def
        .members
        .iter()
        .filter_map(|m| match m {
            ModuleMember::Function(fun)
                if find_attr_slice(&fun.attributes, INIT_ATTR).is_some() =>
            {
                Some(fun)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if init_funs.is_empty() || init_funs.len() > 1 {
        env.add_diag(diag!(
            Derivation::DeriveFailed,
            (
                mod_def.loc,
                "actor module must have exactly one #[init] function".to_string()
            )
        ))
    } else {
        let init_fun = init_funs[0];
        if !init_fun.signature.parameters.is_empty() {
            env.add_diag(diag!(
                Derivation::DeriveFailed,
                (
                    init_fun.loc,
                    "init function must have no arguments".to_string()
                )
            ))
        }
        check_state_param(env, &init_fun.signature.return_type, &state_name);
    }
    state_name
}

/// Checks whether the type is a reference to the actor state.
fn check_state_ref_param(env: &mut CompilationEnv, ty: &Type, struct_name: &Option<StructName>) {
    match &ty.value {
        Type_::Ref(_, state_ty) => {
            check_state_param(env, state_ty, struct_name);
        }
        _ => env.add_diag(diag!(
            Derivation::DeriveFailed,
            (ty.loc, "expected a reference type".to_string())
        )),
    }
}

/// Checks whether the type is the actor state.
fn check_state_param(env: &mut CompilationEnv, state_ty: &Type, struct_name: &Option<StructName>) {
    match &state_ty.value {
        Type_::Apply(access, _) => {
            if let Some(expected_name) = struct_name {
                let given_name = match access.value {
                    NameAccessChain_::One(n) => n,
                    _ => {
                        env.add_diag(diag!(
                            Derivation::DeriveFailed,
                            (
                                access.loc,
                                "qualified type name not supported to reference actor state"
                            )
                        ));
                        return;
                    }
                };
                if given_name.value != expected_name.0.value {
                    env.add_diag(diag!(
                        Derivation::DeriveFailed,
                        (
                            state_ty.loc,
                            format!(
                                "expected actor state type `{}` but found `{}`",
                                expected_name, given_name
                            )
                        )
                    ));
                }
            }
        }
        _ => env.add_diag(diag!(
            Derivation::DeriveFailed,
            (state_ty.loc, "expected a struct type".to_string())
        )),
    }
}

/// Computes a constant expression for the message hash of the given function.
fn message_hash(
    env: &mut CompilationEnv,
    address_map: &NamedAddressMap,
    loc: Loc,
    mod_def: &ModuleDefinition,
    fun_def: &Function,
) -> Exp {
    let addr = match mod_def.address {
        Some(x) => x.value,
        None => {
            env.add_diag(diag!(
                Derivation::DeriveFailed,
                (
                    mod_def.loc,
                    "require explicit module address for async attribute derivation"
                )
            ));
            return sp(loc, Exp_::UnresolvedError);
        }
    };
    let account_addr = match addr {
        LeadingNameAccess_::AnonymousAddress(num) => num.into_inner(),
        LeadingNameAccess_::Name(name) => {
            if let Some(n) = address_map.get(&name.value) {
                n.into_inner()
            } else {
                env.add_diag(diag!(
                    Derivation::DeriveFailed,
                    (loc, format!("cannot resolve address alias `{}`", name))
                ));
                AccountAddress::from_hex_literal("0x0").unwrap()
            }
        }
    };
    let addr_str = format!("0x{:X}", account_addr);
    let hash_str = format!("{}::{}::{}", addr_str, mod_def.name, fun_def.name);
    let hash_bytes: [u8; 8] = Sha3_256::digest(hash_str.as_bytes())[0..8]
        .try_into()
        .expect("valid u64");
    new_u64(loc, u64::from_be_bytes(hash_bytes))
}
