// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    dispatcher_generator::TARGET_CONTRACT_DOES_NOT_CONTAIN_CODE,
    functions::FunctionGenerator,
    native_functions::NativeFunctions,
    solidity_ty::{abi_head_sizes_sum, SignatureDataLocation, SoliditySignature, SolidityType},
    yul_functions::{substitute_placeholders, YulFunction},
};
use itertools::Itertools;
use move_model::{
    emit, emitln,
    model::{FunId, FunctionEnv, Parameter, QualifiedInstId},
    ty::Type,
};
use move_stackless_bytecode::function_target_pipeline::FunctionVariant;
use sha3::{Digest, Keccak256};

impl NativeFunctions {
    /// Generate external functions
    pub(crate) fn define_external_fun(
        &self,
        gen: &mut FunctionGenerator,
        ctx: &Context,
        fun_id: &QualifiedInstId<FunId>,
        solidity_sig_str_opt: Option<String>,
    ) {
        let fun = ctx.env.get_function(fun_id.to_qualified_id());
        let (external_flag, result_ty) = self.check_external_result(ctx, &fun);
        let mut sig = SoliditySignature::create_default_solidity_signature_for_external_fun(
            ctx,
            &fun,
            result_ty.clone(),
        );
        if let Some(solidity_sig_str) = solidity_sig_str_opt {
            let parsed_sig_opt = SoliditySignature::parse_into_solidity_signature(
                ctx,
                &solidity_sig_str,
                &fun,
                &None,
            );
            // Check compatibility
            if let Ok(parsed_sig) = parsed_sig_opt {
                sig = parsed_sig;
            } else if let Err(msg) = parsed_sig_opt {
                ctx.env.error(&fun.get_loc(), &format!("{}", msg));
                return;
            }
        }
        if !sig.check_sig_compatibility_for_external_fun(ctx, &fun, result_ty.clone()) {
            ctx.env.error(
                &fun.get_loc(),
                "solidity signature is not compatible with the move signature",
            );
            return;
        }

        let target = &ctx.targets.get_target(&fun, &FunctionVariant::Baseline);

        // Emit function header
        let params = (0..target.get_parameter_count())
            .map(|idx| ctx.make_local_name(target, idx))
            .join(", ");
        let results = if target.get_return_count() == 0 {
            "".to_string()
        } else {
            (0..target.get_return_count())
                .map(|i| ctx.make_result_name(target, i))
                .join(", ")
        };
        let ret_results = if results.is_empty() {
            "".to_string()
        } else {
            format!(" -> {} ", results)
        };
        emit!(
            ctx.writer,
            "function {}({}){} ",
            ctx.make_function_name(fun_id),
            params,
            ret_results
        );
        let failure_call = gen.parent.call_builtin_str(
            ctx,
            YulFunction::Abort,
            std::iter::once(TARGET_CONTRACT_DOES_NOT_CONTAIN_CODE.to_string()),
        );
        let revert_forward =
            gen.parent
                .call_builtin_str(ctx, YulFunction::RevertForward, vec![].into_iter());
        // Prepare variables used in the function
        let contract_addr_var = ctx.make_local_name(target, 0); // the first parameter is the address of the target contract
        let mut local_name_idx = target.get_parameter_count(); // local variable
        let pos_var = ctx.make_local_name(target, local_name_idx);
        local_name_idx += 1;
        let end_var = ctx.make_local_name(target, local_name_idx);
        local_name_idx += 1;
        let success_var = ctx.make_local_name(target, local_name_idx);
        local_name_idx += 1;
        // Generate the function body
        ctx.emit_block(|| {
            if target.get_return_count() == 0 || ctx.is_unit_opt_ty(result_ty.clone()) {
                // Check extcodesize if no return data is expected
                emitln!(
                    ctx.writer,
                    "if iszero(extcodesize({})) {{ {} }}",
                    contract_addr_var,
                    failure_call
                );
            }
            emitln!(ctx.writer, "// storage for arguments and returned data");
            emitln!(
                ctx.writer,
                "let {} := mload({})",
                pos_var,
                substitute_placeholders("${MEM_SIZE_LOC}").unwrap()
            );

            let fun_sig = format!("{}", sig);
            let function_selector =
                format!("0x{:x}", Keccak256::digest(fun_sig.as_bytes()))[..10].to_string();
            let para_vec = vec![function_selector, "224".to_string()];
            let shl224 =
                gen.parent
                    .call_builtin_str(ctx, YulFunction::Shl, para_vec.iter().cloned());

            emitln!(ctx.writer, "mstore({}, {})", pos_var, shl224);
            let mut encode_params = "".to_string();
            if target.get_parameter_count() > 1 {
                encode_params = format!(
                    ", {}",
                    (1..target.get_parameter_count())
                        .map(|idx| ctx.make_local_name(target, idx))
                        .join(", ")
                );
            }

            let mut para_types = fun.get_parameter_types();
            if para_types.len() > 1 {
                para_types = para_types[1..].to_vec();
            } else {
                para_types = vec![];
            }
            let sig_para_vec = sig
                .para_types
                .clone()
                .into_iter()
                .map(|(ty, _, _)| ty)
                .collect::<Vec<_>>();
            let sig_para_locs = sig
                .para_types
                .clone()
                .into_iter()
                .map(|(_, _, loc)| loc)
                .collect_vec();
            let encode = gen.parent.generate_abi_tuple_encoding(
                ctx,
                sig_para_vec,
                sig_para_locs,
                para_types,
            );
            emitln!(
                ctx.writer,
                "let {} := {}(add({}, 4){})",
                end_var,
                encode,
                pos_var,
                encode_params
            );

            // Make the call
            let mut call = "call".to_string();
            let is_delegatecall = self.is_delegate(ctx, fun_id);
            let is_staticcall = self.is_static(ctx, fun_id);
            if is_delegatecall {
                call = "delegatecall".to_string();
            } else if is_staticcall {
                call = "staticcall".to_string();
            }
            let gas = "gas()".to_string(); // TODO: set gas?
            let mut value = "".to_string();
            if !is_delegatecall && !is_staticcall {
                value = "0, ".to_string(); // TODO: allow sending eth along with making the external call?
            }
            let sig_ret_vec = sig
                .ret_types
                .clone()
                .into_iter()
                .map(|(ty, _)| ty)
                .collect::<Vec<_>>();
            let dynamic_return = self.check_dynamic(&sig_ret_vec);
            let estimated_size = if dynamic_return {
                0
            } else {
                abi_head_sizes_sum(&sig_ret_vec, true)
            };
            emitln!(
                ctx.writer,
                "let {} := {}({}, {}, {} {}, sub({}, {}), {}, {})",
                success_var,
                call,
                gas,
                contract_addr_var,
                value,
                pos_var,
                end_var,
                pos_var,
                pos_var,
                estimated_size
            );
            let return_size = "returndatasize()".to_string();
            emitln!(ctx.writer, "// set freeMemoryPointer");
            emitln!(
                ctx.writer,
                "mstore({}, {})",
                substitute_placeholders("${MEM_SIZE_LOC}").unwrap(),
                end_var
            );
            if !external_flag {
                emitln!(
                    ctx.writer,
                    "if iszero({}) {{ {} }}",
                    success_var,
                    revert_forward
                );
                emit!(ctx.writer, "if {} ", success_var);
            } else {
                emitln!(ctx.writer, "switch iszero({})", success_var);
                emit!(ctx.writer, "case 0 ");
            }
            ctx.emit_block(|| {
                if dynamic_return {
                    emitln!(ctx.writer, "// copy dynamic return data out");
                    emitln!(
                        ctx.writer,
                        "returndatacopy({}, 0, returndatasize())",
                        pos_var
                    );
                }
                emitln!(
                    ctx.writer,
                    "// decode return parameters from external try-call into retVars"
                );
                let fun_ret_types = if external_flag {
                    if ctx.is_unit_opt_ty(result_ty.clone()) {
                        vec![]
                    } else {
                        vec![result_ty.clone().unwrap()]
                    }
                } else {
                    fun.get_return_types()
                };
                if !fun_ret_types.is_empty() {
                    emit!(ctx.writer, "{} := ", results);
                }
                let abi_decode_from_memory =
                    gen.parent
                        .generate_abi_tuple_decoding_ret(ctx, &sig, fun_ret_types, true);
                emitln!(
                    ctx.writer,
                    "{}({}, add({}, {}))",
                    abi_decode_from_memory,
                    pos_var,
                    pos_var,
                    return_size
                );
                if external_flag {
                    let result_0 = ctx.make_result_name(target, 0);
                    if !ctx.is_unit_opt_ty(result_ty.clone()) {
                        self.generate_external_result_fun(
                            gen,
                            ctx,
                            "ok",
                            &result_0,
                            result_ty.clone(),
                        );
                    }
                }
            });
            if external_flag {
                emit!(ctx.writer, "default ");
                let default_failure_var = ctx.make_local_name(target, local_name_idx);
                local_name_idx += 1;
                let result_0 = ctx.make_result_name(target, 0);
                ctx.emit_block(|| {
                    emitln!(ctx.writer, "let {} := 1", default_failure_var);
                    emitln!(
                        ctx.writer,
                        "switch {}",
                        gen.parent.call_builtin_str(
                            ctx,
                            YulFunction::ReturnDataSelector,
                            vec![].into_iter()
                        )
                    );
                    // pack err_reason
                    let selector_reason =
                        format!("0x{:x}", Keccak256::digest("Error(string)".as_bytes()))[..10]
                            .to_string();
                    emit!(ctx.writer, "case {} ", selector_reason);
                    ctx.emit_block(|| {
                        emitln!(
                            ctx.writer,
                            "{} := {}",
                            result_0,
                            gen.parent.call_builtin_str(
                                ctx,
                                YulFunction::TryDecodeErrMsg,
                                vec![].into_iter()
                            )
                        );
                        emit!(ctx.writer, "if {} ", result_0);
                        ctx.emit_block(|| {
                            emitln!(ctx.writer, "{} := 0", default_failure_var);
                            self.generate_external_result_fun(
                                gen,
                                ctx,
                                "err_reason",
                                &result_0,
                                result_ty.clone(),
                            );
                        });
                    });
                    // pack panic
                    let selector_panic =
                        format!("0x{:x}", Keccak256::digest("Panic(uint256)".as_bytes()))[..10]
                            .to_string();
                    emit!(ctx.writer, "case {} ", selector_panic);
                    ctx.emit_block(|| {
                        let panic_success_var = ctx.make_local_name(target, local_name_idx);
                        local_name_idx += 1;
                        let panic_result = ctx.make_local_name(target, local_name_idx);
                        local_name_idx += 1;
                        emitln!(
                            ctx.writer,
                            "let {}, {} := {}",
                            panic_success_var,
                            panic_result,
                            gen.parent.call_builtin_str(
                                ctx,
                                YulFunction::TryDecodePanicData,
                                vec![].into_iter()
                            )
                        );
                        emit!(ctx.writer, "if {} ", panic_success_var);
                        ctx.emit_block(|| {
                            emitln!(ctx.writer, "{} := 0", default_failure_var);
                            emitln!(ctx.writer, "{} := {}", result_0, panic_result);
                            self.generate_external_result_fun(
                                gen,
                                ctx,
                                "panic",
                                &result_0,
                                result_ty.clone(),
                            );
                        });
                    });
                    // pack err_data
                    emit!(ctx.writer, "if {} ", default_failure_var);
                    ctx.emit_block(|| {
                        emitln!(
                            ctx.writer,
                            "{} := {}",
                            result_0,
                            gen.parent.call_builtin_str(
                                ctx,
                                YulFunction::PackErrData,
                                vec![].into_iter()
                            )
                        );
                        self.generate_external_result_fun(
                            gen,
                            ctx,
                            "err_data",
                            &result_0,
                            result_ty.clone(),
                        );
                    });
                });
            }
        });
    }

    fn generate_external_result_fun(
        &self,
        gen: &mut FunctionGenerator,
        ctx: &Context,
        fun_name: &str,
        target_var: &str,
        inst_ty: Option<Type>,
    ) {
        let f_opt = self.find_fun(
            ctx,
            &self.find_module(ctx, "0x2", "ExternalResult"),
            fun_name,
        );
        if let Some(f) = f_opt {
            if let Some(move_ty) = inst_ty {
                let fun_id = &f.instantiate(vec![move_ty]);
                let function_name = ctx.make_function_name(fun_id);
                gen.parent.need_move_function(fun_id);
                emitln!(
                    ctx.writer,
                    "{} := {}({})",
                    target_var,
                    function_name,
                    target_var
                );
            }
        }
    }

    fn check_external_result(&self, ctx: &Context, fun: &FunctionEnv) -> (bool, Option<Type>) {
        if fun.get_return_count() == 1 {
            let ret_type = &fun.get_return_types()[0];
            return ctx.extract_external_result(ret_type);
        }
        (false, None)
    }

    fn check_dynamic(&self, types: &[SolidityType]) -> bool {
        types.iter().any(|a| !a.is_static())
    }

    /// Placeholder for checking whether to make a delegate call
    fn is_delegate(&self, _ctx: &Context, _fun_id: &QualifiedInstId<FunId>) -> bool {
        false
    }

    /// Placeholder for checking whether to make a static call
    fn is_static(&self, _ctx: &Context, _fun_id: &QualifiedInstId<FunId>) -> bool {
        false
    }
}

impl SoliditySignature {
    /// Create a default solidity signature from a move function signature
    pub(crate) fn create_default_solidity_signature_for_external_fun(
        ctx: &Context,
        fun: &FunctionEnv<'_>,
        external_result_ty_opt: Option<Type>,
    ) -> Self {
        let fun_name = fun.symbol_pool().string(fun.get_name()).to_string();
        let mut para_type_lst = vec![];
        if fun.get_parameter_count() < 1 {
            ctx.env.error(
                &fun.get_loc(),
                "external function must have at least one argument",
            );
        } else {
            for Parameter(para_name, move_ty) in fun.get_parameters().into_iter().skip(1) {
                let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, false); // implicit mapping from a move type to a solidity type
                para_type_lst.push((
                    solidity_ty,
                    fun.symbol_pool().string(para_name).to_string(),
                    SignatureDataLocation::Memory, // memory is used by default
                ));
            }
        }

        let mut ret_type_lst = vec![];
        if let Some(move_ty) = external_result_ty_opt {
            if !ctx.is_unit_ty(&move_ty) {
                let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, false);
                ret_type_lst.push((solidity_ty, SignatureDataLocation::Memory));
            }
        } else {
            for move_ty in fun.get_return_types() {
                let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, false);
                ret_type_lst.push((solidity_ty, SignatureDataLocation::Memory));
            }
        }

        SoliditySignature {
            sig_name: fun_name,
            para_types: para_type_lst,
            ret_types: ret_type_lst,
        }
    }

    /// Check whether the user defined solidity signature is compatible with the Move signature
    pub(crate) fn check_sig_compatibility_for_external_fun(
        &self,
        ctx: &Context,
        fun: &FunctionEnv<'_>,
        ty_opt: Option<Type>,
    ) -> bool {
        let para_types = fun.get_parameter_types();
        let sig_para_vec = self
            .para_types
            .iter()
            .map(|(ty, _, _)| ty)
            .collect::<Vec<_>>();
        if para_types.len() != sig_para_vec.len() + 1 {
            // the extra (first) parameter of the move function is the address of the target contract
            return false;
        }
        if !para_types[0].is_address() {
            return false;
        }
        // Check parameter type list
        for type_pair in para_types[1..].iter().zip(sig_para_vec.iter()) {
            let (m_ty, s_ty) = type_pair;
            if !s_ty.check_type_compatibility(ctx, m_ty) {
                return false;
            }
        }
        // Check return type list
        let sig_ret_vec = self.ret_types.iter().map(|(ty, _)| ty).collect::<Vec<_>>();
        if let Some(m_ty) = ty_opt {
            // Special case for checking Unit type
            if sig_ret_vec.is_empty() && ctx.is_unit_ty(&m_ty) {
                return true;
            }
            if sig_ret_vec.len() != 1 || !sig_ret_vec[0].check_type_compatibility(ctx, &m_ty) {
                return false;
            }
        } else {
            let ret_types = fun.get_return_types();
            if ret_types.len() != sig_ret_vec.len() {
                return false;
            }
            for type_pair in ret_types.iter().zip(sig_ret_vec.iter()) {
                let (m_ty, s_ty) = type_pair;
                if !s_ty.check_type_compatibility(ctx, m_ty) {
                    return false;
                }
            }
        }
        true
    }
}
