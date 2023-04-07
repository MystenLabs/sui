// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    functions::FunctionGenerator,
    native_functions::NativeFunctions,
    solidity_ty::{SignatureDataLocation, SoliditySignature, SolidityType},
    yul_functions::{substitute_placeholders, YulFunction},
};
use itertools::Itertools;
use move_model::{
    emit, emitln,
    model::{FunId, FunctionEnv, Parameter, QualifiedInstId},
};
use move_stackless_bytecode::function_target_pipeline::FunctionVariant;

impl NativeFunctions {
    /// Generate decode functions
    pub(crate) fn define_decode_fun(
        &self,
        gen: &mut FunctionGenerator,
        ctx: &Context,
        fun_id: &QualifiedInstId<FunId>,
        solidity_sig_str_opt: Option<String>,
    ) {
        let fun = ctx.env.get_function(fun_id.to_qualified_id());
        let mut sig = SoliditySignature::create_default_sig_for_decode_fun(ctx, &fun);
        if let Some(solidity_sig_str) = solidity_sig_str_opt {
            let parsed_sig_opt = SoliditySignature::parse_into_solidity_signature(
                ctx,
                &solidity_sig_str,
                &fun,
                &None,
            );
            if let Ok(parsed_sig) = parsed_sig_opt {
                sig = parsed_sig;
            } else if let Err(msg) = parsed_sig_opt {
                ctx.env.error(&fun.get_loc(), &format!("{}", msg));
                return;
            }
        }
        // Check compatibility
        if !sig.check_sig_compatibility_for_decode_fun(ctx, &fun) {
            return;
        }
        // Emit function header
        let target = &ctx.targets.get_target(&fun, &FunctionVariant::Baseline);
        let param = (0..target.get_parameter_count())
            .map(|idx| ctx.make_local_name(target, idx))
            .collect_vec();
        assert!(param.len() == 1); // the decode function only has one argument
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
            param[0],
            ret_results
        );
        // Generate the function body
        ctx.emit_block(|| {
            let mut local_name_idx = target.get_parameter_count(); // local variable
            let pos_var = ctx.make_local_name(target, local_name_idx);
            local_name_idx += 1;
            emitln!(ctx.writer, "let {} := add({}, 32)", pos_var, param[0]);
            let size_fun = gen.parent.call_builtin_str(
                ctx,
                YulFunction::MemoryLoadU64,
                std::iter::once(param[0].clone()),
            );
            let size_var = ctx.make_local_name(target, local_name_idx);
            emitln!(ctx.writer, "let {} := {}", size_var, size_fun);
            local_name_idx += 1;
            let offset_var = ctx.make_local_name(target, local_name_idx);
            emitln!(
                ctx.writer,
                "let {} := add({}, {})",
                offset_var,
                pos_var,
                size_var
            );

            let fun_ret_types = fun.get_return_types();
            let abi_decode_from_memory =
                gen.parent
                    .generate_abi_tuple_decoding_ret(ctx, &sig, fun_ret_types.clone(), true);

            emitln!(
                ctx.writer,
                "if gt({}, 0xffffffffffffffff) {{ {} }}",
                pos_var,
                gen.parent
                    .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
            );
            emitln!(
                ctx.writer,
                "if gt({}, 0xffffffffffffffff) {{ {} }}",
                offset_var,
                gen.parent
                    .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
            );

            if !fun_ret_types.is_empty() {
                emit!(ctx.writer, "{} := ", results);
            }
            emitln!(
                ctx.writer,
                "{}({}, {})",
                abi_decode_from_memory,
                pos_var,
                offset_var
            );
        });
    }

    /// Generate encode functions
    pub(crate) fn define_encode_fun(
        &self,
        gen: &mut FunctionGenerator,
        ctx: &Context,
        fun_id: &QualifiedInstId<FunId>,
        solidity_sig_str_opt: Option<String>,
        packed_flag: bool,
    ) {
        let fun = ctx.env.get_function(fun_id.to_qualified_id());
        let mut sig = SoliditySignature::create_default_sig_for_encode_fun(ctx, &fun);
        if let Some(solidity_sig_str) = solidity_sig_str_opt {
            let parsed_sig_opt = SoliditySignature::parse_into_solidity_signature(
                ctx,
                &solidity_sig_str,
                &fun,
                &None,
            );
            if let Ok(parsed_sig) = parsed_sig_opt {
                sig = parsed_sig;
            } else if let Err(msg) = parsed_sig_opt {
                ctx.env.error(&fun.get_loc(), &format!("{}", msg));
                return;
            }
        }
        // Check compatibility
        if !sig.check_sig_compatibility_for_encode_fun(ctx, &fun) {
            return;
        }
        // Emit function header
        let target = &ctx.targets.get_target(&fun, &FunctionVariant::Baseline);
        let params = (0..target.get_parameter_count())
            .map(|idx| ctx.make_local_name(target, idx))
            .join(",");
        let results = (0..target.get_return_count())
            .map(|i| ctx.make_result_name(target, i))
            .collect_vec();
        assert!(results.len() == 1); // the encode function only has one return value
        let ret_results = format!(" -> {} ", results[0]);
        emit!(
            ctx.writer,
            "function {}({}){} ",
            ctx.make_function_name(fun_id),
            params,
            ret_results
        );
        // Generate the function body
        ctx.emit_block(|| {
            emitln!(
                ctx.writer,
                "{} := mload({})",
                results[0],
                substitute_placeholders("${MEM_SIZE_LOC}").unwrap()
            );
            let mut local_name_idx = target.get_parameter_count();
            let encode_start_var = ctx.make_local_name(target, local_name_idx);
            local_name_idx += 1;
            emitln!(
                ctx.writer,
                "let {} := add({}, 32)",
                encode_start_var,
                results[0]
            );
            emitln!(
                ctx.writer,
                "if gt({}, 0xffffffffffffffff) {{ {} }}",
                encode_start_var,
                gen.parent
                    .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
            );

            let encode_end_var = ctx.make_local_name(target, local_name_idx);
            local_name_idx += 1;

            let fun_para_types = fun.get_parameter_types();
            let abi_encode =
                gen.parent
                    .generate_abi_tuple_encoding_para(ctx, &sig, fun_para_types, packed_flag);
            emit!(ctx.writer, "let {} := ", encode_end_var);
            let args = if params.is_empty() {
                "".to_string()
            } else {
                format!(",{}", params)
            };
            emitln!(ctx.writer, "{}({}{})", abi_encode, encode_start_var, args);
            emitln!(
                ctx.writer,
                "if gt({}, 0xffffffffffffffff) {{ {} }}",
                encode_end_var,
                gen.parent
                    .call_builtin_str(ctx, YulFunction::AbortBuiltin, std::iter::empty())
            );
            let encode_size_var = ctx.make_local_name(target, local_name_idx);
            local_name_idx += 1;
            emitln!(
                ctx.writer,
                "let {} := sub({}, {})",
                encode_size_var,
                encode_end_var,
                encode_start_var
            );
            // store the current length of vector
            gen.parent.call_builtin(
                ctx,
                YulFunction::MemoryStoreU64,
                vec![results[0].clone(), encode_size_var.clone()].into_iter(),
            );

            // store the capacity of vector
            let encode_capacity_var = ctx.make_local_name(target, local_name_idx);
            let capacity_call = gen.parent.call_builtin_str(
                ctx,
                YulFunction::ClosestGreaterPowerOfTwo,
                std::iter::once(encode_size_var),
            );
            emitln!(
                ctx.writer,
                "let {} := {}",
                encode_capacity_var,
                capacity_call
            );
            gen.parent.call_builtin(
                ctx,
                YulFunction::MemoryStoreU64,
                vec![format!("add({}, 8)", results[0]), encode_capacity_var].into_iter(),
            );
            emitln!(
                ctx.writer,
                "mstore({}, {})",
                substitute_placeholders("${MEM_SIZE_LOC}").unwrap(),
                encode_end_var
            );
        });
    }
}

impl SoliditySignature {
    pub(crate) fn create_default_sig_for_decode_fun(ctx: &Context, fun: &FunctionEnv<'_>) -> Self {
        let fun_name = fun.symbol_pool().string(fun.get_name()).to_string();
        let mut para_type_lst = vec![];
        for Parameter(para_name, move_ty) in fun.get_parameters() {
            let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, true);
            para_type_lst.push((
                solidity_ty,
                fun.symbol_pool().string(para_name).to_string(),
                SignatureDataLocation::Memory,
            ));
        }
        let mut ret_type_lst = vec![];

        for move_ty in fun.get_return_types() {
            let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, false);
            ret_type_lst.push((solidity_ty, SignatureDataLocation::Memory));
        }

        SoliditySignature {
            sig_name: fun_name,
            para_types: para_type_lst,
            ret_types: ret_type_lst,
        }
    }

    pub(crate) fn create_default_sig_for_encode_fun(ctx: &Context, fun: &FunctionEnv<'_>) -> Self {
        let fun_name = fun.symbol_pool().string(fun.get_name()).to_string();
        let mut para_type_lst = vec![];
        for Parameter(para_name, move_ty) in fun.get_parameters() {
            let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, false);
            para_type_lst.push((
                solidity_ty,
                fun.symbol_pool().string(para_name).to_string(),
                SignatureDataLocation::Memory,
            ));
        }
        let mut ret_type_lst = vec![];

        for move_ty in fun.get_return_types() {
            let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, true);
            ret_type_lst.push((solidity_ty, SignatureDataLocation::Memory));
        }

        SoliditySignature {
            sig_name: fun_name,
            para_types: para_type_lst,
            ret_types: ret_type_lst,
        }
    }

    /// Check whether the user defined solidity signature is compatible with the Move signature
    pub(crate) fn check_sig_compatibility_for_decode_fun(
        &self,
        ctx: &Context,
        fun: &FunctionEnv<'_>,
    ) -> bool {
        if !self.check_sig_compatibility(ctx, fun, &None) {
            ctx.env.error(
                &fun.get_loc(),
                "solidity signature is not compatible with the move signature",
            );
            return false;
        }
        let sig_para_vec = self
            .para_types
            .iter()
            .map(|(ty, _, _)| ty)
            .collect::<Vec<_>>();
        if sig_para_vec.len() == 1 {
            if let SolidityType::Bytes = sig_para_vec[0] {
                // the only argument must be of type bytes (vector<u8>)
                return true;
            }
        }
        ctx.env.error(
            &fun.get_loc(),
            "decode function must only has one argument of type bytes",
        );
        false
    }

    /// Check whether the user defined solidity signature is compatible with the Move signature
    pub(crate) fn check_sig_compatibility_for_encode_fun(
        &self,
        ctx: &Context,
        fun: &FunctionEnv<'_>,
    ) -> bool {
        if !self.check_sig_compatibility(ctx, fun, &None) {
            ctx.env.error(
                &fun.get_loc(),
                "solidity signature is not compatible with the move signature",
            );
            return false;
        }
        let sig_ret_vec = self.ret_types.iter().map(|(ty, _)| ty).collect::<Vec<_>>();
        if sig_ret_vec.len() == 1 {
            if let SolidityType::Bytes = sig_ret_vec[0] {
                // the only return value must be of type bytes (vector<u8>)
                return true;
            }
        }
        ctx.env.error(
            &fun.get_loc(),
            "encode function must only has one return value of type bytes",
        );
        false
    }
}
