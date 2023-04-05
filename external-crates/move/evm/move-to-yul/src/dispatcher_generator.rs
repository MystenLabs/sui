// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::diagnostic::Severity;
use std::collections::BTreeMap;

use itertools::Itertools;
use regex::Regex;
use sha3::{Digest, Keccak256};

use move_model::{
    ast::TempIndex,
    emit, emitln,
    model::{FunId, FunctionEnv, QualifiedId, QualifiedInstId, StructId},
    ty::{PrimitiveType, Type},
};

use crate::{
    attributes,
    context::Context,
    solidity_ty::{
        abi_head_sizes_sum, abi_head_sizes_vec, mangle_solidity_types, SignatureDataLocation,
        SoliditySignature, SolidityType,
    },
    vectors::VECTOR_METADATA_SIZE,
    yul_functions::{substitute_placeholders, YulFunction},
    Generator,
};

// Revert reasons
pub const REVERT_ERR_NON_PAYABLE_FUN: usize = 99;
pub const UNKNOWN_SIGNATURE_AND_NO_FALLBACK_DEFINED: usize = 98;
pub const NO_RECEIVE_OR_FALLBACK_FUN: usize = 97;
pub const ABI_DECODING_DATA_TOO_SHORT: usize = 96;
pub const ABI_DECODING_PARAM_VALIDATION: usize = 95;
pub const ABI_DECODING_INVALID_CALLDATA_ARRAY_OFFSET: usize = 94;
pub const ABI_DECODING_INVALID_BYTE_ARRAY_OFFSET: usize = 93;
pub const STATIC_ARRAY_SIZE_NOT_MATCH: usize = 92;
pub const TARGET_CONTRACT_DOES_NOT_CONTAIN_CODE: usize = 91;
pub const ABI_DECODING_STRUCT_DATA_TOO_SHORT: usize = 90;

#[derive(Debug, Clone)]
pub(crate) struct EncodingOptions {
    pub padded: bool,
    pub in_place: bool,
}

impl EncodingOptions {
    pub(crate) fn to_suffix(&self) -> String {
        let mut result = "".to_string();
        if !self.padded {
            result = format!("{}_not_padded", result);
        }
        if self.in_place {
            result = format!("{}_inplace", result);
        }
        result
    }
}

impl Generator {
    /// Generate dispatcher routine
    pub(crate) fn generate_dispatcher_routine(
        &mut self,
        ctx: &Context,
        callables: &[FunctionEnv<'_>],
        receiver: &Option<FunctionEnv<'_>>,
        fallback: &Option<FunctionEnv<'_>>,
    ) {
        emitln!(ctx.writer, "if iszero(lt(calldatasize(), 4))");
        let mut selectors = BTreeMap::new();
        let para_vec = vec!["calldataload(0)".to_string(), "224".to_string()];
        let shr224 = self.call_builtin_str(ctx, YulFunction::Shr, para_vec.iter().cloned());
        ctx.emit_block(|| {
            emitln!(ctx.writer, "let selector := {}", shr224);
            emitln!(ctx.writer, "switch selector");
            for fun in callables {
                if !self.is_suitable_for_dispatch(ctx, fun) {
                    ctx.env.diag(
                        Severity::Warning,
                        &fun.get_loc(),
                        "cannot dispatch this function because of unsupported parameter types",
                    );
                    continue;
                }
                let sig = self.get_solidity_signature(ctx, fun, true);
                if let Some(fun_attr_opt) = attributes::construct_fun_attribute(fun) {
                    self.solidity_sigs.push((sig.clone(), fun_attr_opt));
                } else {
                    ctx.env.error(
                        &fun.get_loc(),
                        "callable functions can only have one attribute among payable, pure and view",
                    );
                }
                self.generate_dispatch_item(ctx, fun, &sig, &mut selectors);
            }
            emitln!(ctx.writer, "default {}");
        });
        let receive_exists = self.optional_receive(ctx, receiver);
        self.generate_fallback(ctx, receive_exists, fallback);
    }

    /// Returns the Solidity signature of the given function.
    pub(crate) fn get_solidity_signature(
        &self,
        ctx: &Context,
        fun: &FunctionEnv,
        callable_flag: bool,
    ) -> SoliditySignature {
        let extracted_sig_opt =
            attributes::extract_callable_or_create_signature(fun, callable_flag);
        let mut sig =
            SoliditySignature::create_default_solidity_signature(ctx, fun, &self.storage_type);
        if let Some(extracted_sig) = extracted_sig_opt {
            let parsed_sig_opt = SoliditySignature::parse_into_solidity_signature(
                ctx,
                &extracted_sig,
                fun,
                &self.storage_type,
            );
            if let Ok(parsed_sig) = parsed_sig_opt {
                if !parsed_sig.check_sig_compatibility(ctx, fun, &self.storage_type) {
                    ctx.env.error(
                        &fun.get_loc(),
                        "solidity signature is not compatible with the move signature",
                    );
                } else {
                    sig = parsed_sig;
                }
            } else if let Err(msg) = parsed_sig_opt {
                ctx.env.error(&fun.get_loc(), &format!("{}", msg));
            }
        }
        sig
    }

    fn generate_dispatch_item(
        &mut self,
        ctx: &Context,
        fun: &FunctionEnv<'_>,
        solidity_sig: &SoliditySignature,
        selectors: &mut BTreeMap<String, QualifiedId<FunId>>,
    ) {
        let fun_id = &fun.get_qualified_id().instantiate(vec![]);
        let function_name = ctx.make_function_name(fun_id);
        let fun_sig = format!("{}", solidity_sig);
        self.need_move_function(fun_id);
        let function_selector =
            format!("0x{:x}", Keccak256::digest(fun_sig.as_bytes()))[..10].to_string();
        // Check selector collision
        if let Some(other_fun) = selectors.insert(function_selector.clone(), fun.get_qualified_id())
        {
            ctx.env.error(
                &fun.get_loc(),
                &format!(
                    "hash collision for function selector with `{}`",
                    ctx.env.get_function(other_fun).get_full_name_str()
                ),
            );
        }
        emitln!(ctx.writer, "case {}", function_selector);
        ctx.emit_block(|| {
            emitln!(ctx.writer, "// {}", fun_sig);
            // TODO: check delegate call
            if !attributes::is_payable_fun(fun) {
                self.generate_call_value_check(ctx, REVERT_ERR_NON_PAYABLE_FUN);
            }
            // Decoding
            let mut logical_param_types = fun.get_parameter_types();
            let storage_type = if !logical_param_types.is_empty()
                && ctx.is_storage_ref(&self.storage_type, &logical_param_types[0])
            {
                // Skip the storage reference parameter.
                logical_param_types.remove(0);
                Some(self.storage_type.clone().unwrap())
            } else {
                None
            };
            let param_count = solidity_sig.para_types.len();
            let mut params = "".to_string();
            if param_count > 0 {
                let decoding_fun_name = self.generate_abi_tuple_decoding_para(
                    ctx,
                    solidity_sig,
                    logical_param_types,
                    false,
                );
                params = (0..param_count).map(|i| format!("param_{}", i)).join(", ");
                let let_params = format!("let {} := ", params);
                emitln!(
                    ctx.writer,
                    "{}{}(4, calldatasize())",
                    let_params,
                    decoding_fun_name
                );
            }
            let ret_count = solidity_sig.ret_types.len();
            let mut rets = "".to_string();
            let mut let_rets = "".to_string();
            if ret_count > 0 {
                rets = (0..ret_count).map(|i| format!("ret_{}", i)).join(", ");
                let_rets = format!("let {} := ", rets);
            }
            // Add optional storage ref parameter
            params = self.add_storage_ref_param(ctx, &storage_type, params);
            // Call the function
            emitln!(ctx.writer, "{}{}({})", let_rets, function_name, params);
            // Encoding the return values
            let encoding_fun_name =
                self.generate_abi_tuple_encoding_ret(ctx, solidity_sig, fun.get_return_types());
            if ret_count > 0 {
                rets = format!(", {}", rets);
            }
            // Prepare the return values
            self.generate_allocate_unbounded(ctx);
            emitln!(
                ctx.writer,
                "let memEnd := {}(memPos{})",
                encoding_fun_name,
                rets
            );
            emitln!(ctx.writer, "return(memPos, sub(memEnd, memPos))");
        });
    }

    /// Adds the optional storage ref parameter to a parameter list.
    fn add_storage_ref_param(
        &mut self,
        ctx: &Context,
        storage_type: &Option<QualifiedInstId<StructId>>,
        params: String,
    ) -> String {
        if let Some(storage) = storage_type {
            // The first parameter is a reference to the storage struct.
            let storage_ref = self.borrow_global_instrs(ctx, storage, "address()".to_string());
            if params.is_empty() {
                storage_ref
            } else {
                vec![storage_ref, params].into_iter().join(", ")
            }
        } else {
            params
        }
    }

    /// Determine whether the function is suitable as a dispatcher item.
    pub(crate) fn is_suitable_for_dispatch(&self, ctx: &Context, fun: &FunctionEnv) -> bool {
        let mut types = fun.get_parameter_types();
        if !types.is_empty() && ctx.is_storage_ref(&self.storage_type, &types[0]) {
            // Skip storage ref parameter
            types.remove(0);
        }
        if !attributes::is_create_fun(fun) || self.storage_type.is_none() {
            // If this is not a creator which returns a storage value, add return types.
            types.extend(fun.get_return_types().into_iter())
        }
        types.into_iter().all(|ty| !ty.is_reference())
    }

    /// Generate optional receive function.
    fn optional_receive(&mut self, ctx: &Context, receive: &Option<FunctionEnv<'_>>) -> bool {
        if let Some(receive) = receive {
            ctx.check_no_generics(receive);
            if !attributes::is_payable_fun(receive) {
                ctx.env
                    .error(&receive.get_loc(), "receive function must be payable")
            }
            let mut param_count = receive.get_parameter_count();
            let storage_type = if param_count > 0
                && ctx.is_storage_ref(&self.storage_type, &receive.get_local_type(0))
            {
                param_count -= 1;
                Some(self.storage_type.clone().unwrap())
            } else {
                None
            };
            if param_count > 0 {
                ctx.env.error(
                    &receive.get_loc(),
                    "receive function must not have parameters in addition to optional storage reference",
                )
            }
            let fun_id = &receive
                .module_env
                .get_id()
                .qualified(receive.get_id())
                .instantiate(vec![]);
            ctx.emit_block(|| {
                let params = self.add_storage_ref_param(ctx, &storage_type, "".to_string());
                emitln!(
                    ctx.writer,
                    "if iszero(calldatasize()) {{ {}({}) stop() }}",
                    ctx.make_function_name(fun_id),
                    params
                );
            });
            true
        } else {
            false
        }
    }

    /// Generate fallback function.
    fn generate_fallback(
        &mut self,
        ctx: &Context,
        receive_ether: bool,
        fallback: &Option<FunctionEnv<'_>>,
    ) {
        if let Some(fallback) = fallback {
            ctx.check_no_generics(fallback);
            if !attributes::is_payable_fun(fallback) {
                self.generate_call_value_check(ctx, REVERT_ERR_NON_PAYABLE_FUN);
            }
            let fun_id = &fallback
                .module_env
                .get_id()
                .qualified(fallback.get_id())
                .instantiate(vec![]);
            let fun_name = ctx.make_function_name(fun_id);
            let mut param_count = fallback.get_parameter_count();
            let storage_type = if param_count > 0
                && ctx.is_storage_ref(&self.storage_type, &fallback.get_local_type(0))
            {
                param_count -= 1;
                Some(self.storage_type.clone().unwrap())
            } else {
                None
            };
            ctx.emit_block(|| {
                let mut params = self.add_storage_ref_param(ctx, &storage_type, "".to_string());
                if param_count == 0 {
                    emitln!(ctx.writer, "{}({}) stop()", fun_name, params);
                } else if param_count != 1 || fallback.get_return_count() != 1 {
                    ctx.env.error(
                        &fallback.get_loc(),
                        "fallback function must have at most 1 parameter and 1 return value",
                    );
                } else {
                    if !params.is_empty() {
                        params = format!("{}, ", params);
                    }
                    emitln!(
                        ctx.writer,
                        "let retval := {}({}0, calldatasize()) stop()",
                        fun_name,
                        params
                    );
                    emitln!(ctx.writer, "return(add(retval, 0x20), mload(retval))");
                }
            })
        } else {
            let mut err_msg = NO_RECEIVE_OR_FALLBACK_FUN;
            if receive_ether {
                err_msg = UNKNOWN_SIGNATURE_AND_NO_FALLBACK_DEFINED;
            }
            self.call_builtin(
                ctx,
                YulFunction::Abort,
                std::iter::once(err_msg.to_string()),
            );
        }
    }

    /// Generate the code to check value
    fn generate_call_value_check(&mut self, ctx: &Context, err_code: TempIndex) {
        emitln!(ctx.writer, "if callvalue()");
        ctx.emit_block(|| {
            self.call_builtin(
                ctx,
                YulFunction::Abort,
                std::iter::once(err_code.to_string()),
            );
        });
    }

    /// Generate the start position of memory for returning from the external function
    /// Note: currently, we directly return the free memory pointer, may need to use the memory model later
    fn generate_allocate_unbounded(&mut self, ctx: &Context) {
        emitln!(
            ctx.writer,
            "let memPos := mload({})",
            substitute_placeholders("${MEM_SIZE_LOC}").unwrap()
        );
    }

    /// Generate the cleanup function used in the validator and the encoding function.
    fn generate_cleanup(&mut self, ty: &SolidityType) -> String {
        let name_prefix = "cleanup";
        let function_name = format!("{}_{}", name_prefix, ty);
        let mask = ty.max_value();

        let generate_fun = move |_gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value) -> cleaned ");
            ctx.emit_block(|| emitln!(ctx.writer, "cleaned := and(value, {})", mask));
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    fn generate_left_align(&mut self, ty: &SolidityType) -> String {
        use crate::solidity_ty::{SolidityPrimitiveType::*, SolidityType::*};
        assert!(ty.is_value_type());
        let ty = ty.clone();
        let name_prefix = "left_align";
        let function_name = format!("{}_{}", name_prefix, ty);
        let generate_fun = move |_gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value) -> aligned ");
            ctx.emit_block(|| {
                let bits = 256
                    - match ty {
                        Primitive(p) => match p {
                            Bool => 8,
                            Int(size) | Uint(size) => size,
                            Address(_) => 160,
                            _ => panic!("wrong types"),
                        },
                        BytesStatic(_) => 256,
                        _ => panic!("wrong types"),
                    };
                if bits > 0 {
                    emitln!(ctx.writer, "aligned := shl({}, value)", bits);
                }
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate the validator function, which is used in the decode function.
    fn generate_validator(&mut self, ty: &SolidityType) -> String {
        let name_prefix = "validator";
        let function_name = format!("{}_{}", name_prefix, ty);
        let ty = ty.clone(); // need to move into lambda

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value) ");
            ctx.emit_block(|| {
                let condition = format!("eq(value, {}(value))", gen.generate_cleanup(&ty));
                let failure_call = gen.call_builtin_str(
                    ctx,
                    YulFunction::Abort,
                    std::iter::once(ABI_DECODING_PARAM_VALIDATION.to_string()),
                );
                emitln!(
                    ctx.writer,
                    "if iszero({}) {{ {} }}",
                    condition,
                    failure_call
                );
            })
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate decoding functions for solidity structs.
    fn generate_abi_decoding_struct_type(
        &mut self,
        ctx: &Context,
        ty: &SolidityType,
        move_ty: &Type,
        from_memory: bool,
    ) -> String {
        use SolidityType::*;
        assert!(matches!(ty, Struct(_, _)), "wrong types");
        let name_prefix = "abi_decode";
        let from_memory_str = if from_memory { "_from_memory" } else { "" };

        let mut param_types = vec![];
        let mut move_tys = vec![];
        let mut real_offsets = vec![];
        if let Struct(_, ty_tuples) = ty {
            param_types = ty_tuples
                .iter()
                .map(|(_, _, _, _, ty)| ty.clone())
                .collect_vec();
            move_tys = ty_tuples
                .iter()
                .map(|(_, _, m_ty, _, _)| m_ty.clone())
                .collect_vec();
            real_offsets = ty_tuples
                .iter()
                .map(|(_, real_offset, _, _, _)| *real_offset)
                .collect_vec();
        }

        let mut function_name = format!(
            "{}_{}_{}{}",
            name_prefix,
            mangle_solidity_types(&param_types),
            ctx.mangle_types(&[move_ty.clone()]),
            from_memory_str
        );
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            let overall_type_head_vec = abi_head_sizes_vec(&param_types, true);
            let overall_type_head_size = abi_head_sizes_sum(&param_types, true);
            let ret_var = (0..overall_type_head_vec.len())
                .map(|i| format!("value_{}", i))
                .collect_vec();
            emit!(ctx.writer, "(headStart, end) -> value ");
            ctx.emit_block(|| {
                let failure_call = gen.call_builtin_str(
                    ctx,
                    YulFunction::Abort,
                    std::iter::once(ABI_DECODING_STRUCT_DATA_TOO_SHORT.to_string()),
                );
                let malloc = gen.call_builtin_str(
                    ctx,
                    YulFunction::Malloc,
                    std::iter::once(overall_type_head_size.to_string()), // Make sure the allocated size is not over the limit
                );
                emitln!(
                    ctx.writer,
                    "if slt(sub(end, headStart), {}) {{ {} }}",
                    overall_type_head_size,
                    failure_call
                );
                emitln!(ctx.writer, "let {}", ret_var.iter().join(", "));
                emitln!(ctx.writer, "value := {}", malloc);
                assert!(real_offsets.len() == overall_type_head_vec.len());
                let mut head_pos = 0;
                for (stack_pos, (((ty, ty_size), move_ty), real_offset)) in overall_type_head_vec
                    .iter()
                    .zip(move_tys.iter())
                    .zip(real_offsets.iter())
                    .enumerate()
                {
                    let is_static = ty.is_static();
                    let local_typ_var = vec![ret_var[stack_pos].clone()];
                    let abi_decode_type =
                        gen.generate_abi_decoding_type(ctx, ty, move_ty, from_memory);
                    ctx.emit_block(|| {
                        if is_static {
                            emitln!(ctx.writer, "let offset := {}", head_pos);
                        } else {
                            let load = if from_memory { "mload" } else { "calldataload" };
                            emitln!(
                                ctx.writer,
                                "let offset := {}(add(headStart, {}))",
                                load,
                                head_pos
                            );
                            emitln!(
                                ctx.writer,
                                "if gt(offset, 0xffffffffffffffff) {{ {} }}",
                                gen.call_builtin_str(
                                    ctx,
                                    YulFunction::Abort,
                                    std::iter::once(ABI_DECODING_DATA_TOO_SHORT.to_string())
                                )
                            );
                        }
                        emitln!(
                            ctx.writer,
                            "{} := {}(add(headStart, offset), end)",
                            local_typ_var.iter().join(", "),
                            abi_decode_type
                        );
                    });
                    head_pos += ty_size;
                    let memory_func = ctx.memory_store_builtin_fun(move_ty);
                    if local_typ_var.len() == 1 {
                        gen.call_builtin(
                            ctx,
                            memory_func,
                            vec![
                                format!("add(value, {})", real_offset),
                                local_typ_var[0].clone(),
                            ]
                            .into_iter(),
                        );
                    }
                }
            })
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate decoding functions for tuple.
    pub(crate) fn generate_abi_tuple_decoding(
        &mut self,
        ctx: &Context,
        param_types: Vec<SolidityType>,
        param_locs: Vec<SignatureDataLocation>,
        move_tys: Vec<Type>,
        from_memory: bool,
    ) -> String {
        let name_prefix = "abi_decode_tuple";
        let from_memory_str = if from_memory { "_from_memory" } else { "" };
        let mut function_name = format!(
            "{}_{}_{}{}",
            name_prefix,
            mangle_solidity_types(&param_types),
            ctx.mangle_types(&move_tys),
            from_memory_str
        );
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();
        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            let overall_type_head_vec = abi_head_sizes_vec(&param_types, true);
            let overall_type_head_size = abi_head_sizes_sum(&param_types, true);

            let ret_var = (0..overall_type_head_vec.len())
                .map(|i| format!("value_{}", i))
                .collect_vec();
            let ret_var_str = if ret_var.is_empty() {
                "".to_string()
            } else {
                format!(" -> {}", ret_var.iter().join(", "))
            };
            emit!(ctx.writer, "(headStart, dataEnd){} ", ret_var_str);
            ctx.emit_block(|| {
                emitln!(
                    ctx.writer,
                    "if slt(sub(dataEnd, headStart), {}) {{ {} }}",
                    overall_type_head_size,
                    gen.call_builtin_str(
                        ctx,
                        YulFunction::Abort,
                        std::iter::once(ABI_DECODING_DATA_TOO_SHORT.to_string())
                    ),
                );
                let mut head_pos = 0;
                for (stack_pos, (((ty, ty_size), _loc), move_ty)) in overall_type_head_vec
                    .iter()
                    .zip(param_locs.iter())
                    .zip(move_tys.iter())
                    .enumerate()
                {
                    let is_static = ty.is_static();
                    // TODO: consider the case size_on_stack is not 1
                    let local_typ_var = vec![ret_var[stack_pos].clone()];
                    let abi_decode_type =
                        gen.generate_abi_decoding_type(ctx, ty, move_ty, from_memory);
                    ctx.emit_block(|| {
                        if is_static {
                            emitln!(ctx.writer, "let offset := {}", head_pos);
                        } else {
                            // TODO: dynamic types need to be revisited
                            let load = if from_memory { "mload" } else { "calldataload" };
                            emitln!(
                                ctx.writer,
                                "let offset := {}(add(headStart, {}))",
                                load,
                                head_pos
                            );
                            emitln!(
                                ctx.writer,
                                "if gt(offset, 0xffffffffffffffff) {{ {} }}",
                                gen.call_builtin_str(
                                    ctx,
                                    YulFunction::Abort,
                                    std::iter::once(ABI_DECODING_DATA_TOO_SHORT.to_string())
                                )
                            );
                        }
                        emitln!(
                            ctx.writer,
                            "{} := {}(add(headStart, offset), dataEnd)",
                            local_typ_var.iter().join(", "),
                            abi_decode_type
                        );
                    });
                    head_pos += ty_size;
                }
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    pub(crate) fn generate_abi_tuple_decoding_ret(
        &mut self,
        ctx: &Context,
        sig: &SoliditySignature,
        move_tys: Vec<Type>,
        from_memory: bool,
    ) -> String {
        let param_types = sig.ret_types.iter().map(|(ty, _)| ty.clone()).collect_vec(); // need to move into lambda
        let ret_locs = sig
            .ret_types
            .iter()
            .map(|(_, loc)| loc.clone())
            .collect_vec();
        self.generate_abi_tuple_decoding(ctx, param_types, ret_locs, move_tys, from_memory)
    }

    pub(crate) fn generate_abi_tuple_decoding_para(
        &mut self,
        ctx: &Context,
        sig: &SoliditySignature,
        move_tys: Vec<Type>,
        from_memory: bool,
    ) -> String {
        let param_types = sig
            .para_types
            .iter()
            .map(|(ty, _, _)| ty.clone())
            .collect_vec(); // need to move into lambda
        let ret_locs = sig
            .para_types
            .iter()
            .map(|(_, _, loc)| loc.clone())
            .collect_vec();
        self.generate_abi_tuple_decoding(ctx, param_types, ret_locs, move_tys, from_memory)
    }

    /// Generate decoding functions for ty.
    fn generate_abi_decoding_type(
        &mut self,
        ctx: &Context,
        ty: &SolidityType,
        move_ty: &Type,
        from_memory: bool,
    ) -> String {
        use SolidityType::*;
        // TODO: struct
        match ty {
            Primitive(_) => self.generate_abi_decoding_primitive_type(ty, from_memory),
            DynamicArray(_) | StaticArray(_, _) | Bytes | BytesStatic(_) | SolidityString => {
                self.generate_abi_decoding_array_type(ctx, ty, move_ty, from_memory)
            }
            Struct(_, _) => self.generate_abi_decoding_struct_type(ctx, ty, move_ty, from_memory),
            _ => "".to_string(),
        }
    }

    /// Generate decoding functions for primitive types.
    fn generate_abi_decoding_primitive_type(
        &mut self,
        ty: &SolidityType,
        from_memory: bool,
    ) -> String {
        let name_prefix = "abi_decode";
        let from_memory_str = if from_memory { "_from_memory" } else { "" };
        let function_name = format!("{}_{}{}", name_prefix, ty, from_memory_str);
        let ty = ty.clone(); // need to move into lambda

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(offset, end) -> value ");
            let load = if from_memory { "mload" } else { "calldataload" };
            ctx.emit_block(|| {
                emitln!(ctx.writer, "value := {}(offset)", load);
                let validator = gen.generate_validator(&ty);
                emitln!(ctx.writer, "{}(value)", validator);
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Aux function to compute the length of an array and the size to be allocated in the memory for a fixed-sized array
    fn compute_static_array_type_length_size(
        &mut self,
        ctx: &Context,
        ty: &SolidityType,
        inner_move_ty: &Type,
    ) -> (usize, usize) {
        use SolidityType::*;
        match ty {
            BytesStatic(len) | StaticArray(_, len) => (
                *len,
                VECTOR_METADATA_SIZE + ctx.type_size(inner_move_ty) * len,
            ),
            _ => panic!("wrong types"),
        }
    }

    /// Generate decoding function for array types including static and dynamic arrays, static and dynamic bytes and string
    fn generate_abi_decoding_array_type(
        &mut self,
        ctx: &Context,
        ty: &SolidityType,
        move_ty: &Type,
        from_memory: bool,
    ) -> String {
        let name_prefix = "abi_decode";
        let from_memory_str = if from_memory { "_from_memory" } else { "" };
        let mut function_name = format!(
            "{}_{}_{}{}",
            name_prefix,
            ty,
            ctx.mangle_type(move_ty),
            from_memory_str
        );
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();
        let ty = ty.clone(); // need to move into lambda
        let move_ty = move_ty.clone();

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(offset, end) -> array ");
            let failure_call = gen.call_builtin_str(
                ctx,
                YulFunction::Abort,
                std::iter::once(ABI_DECODING_INVALID_CALLDATA_ARRAY_OFFSET.to_string()),
            );
            let allocated_size; // memory size to be allocated for the current array
            let array_length; // length of the current array
            let inner_move_ty = match move_ty {
                Type::Vector(ref _ty) => _ty.clone(),
                Type::Struct(mid, sid, _) => {
                    let st_id = mid.qualified(sid);
                    if ctx.is_string(st_id) {
                        Box::new(Type::Primitive(PrimitiveType::U8))
                    } else {
                        panic!("wrong type")
                    }
                }
                _ => panic!("wrong type"),
            };
            let mut offset = "add(offset, 0x20)";
            if ty.is_array_static_size() {
                // current array has fixed number of elements
                let (len, size) =
                    gen.compute_static_array_type_length_size(ctx, &ty, &inner_move_ty);
                array_length = format!("{}", len);
                allocated_size = format!("{}", size);
                offset = "offset";
            } else {
                // if the size is not fixed, get the length from offset in calldata
                let load = if from_memory { "mload" } else { "calldataload" };
                array_length = format!("{}(offset)", load);
                allocated_size = format!(
                    "add(mul({}, length), {})",
                    ctx.type_size(&inner_move_ty),
                    VECTOR_METADATA_SIZE
                );
            }
            ctx.emit_block(|| {
                emitln!(
                    ctx.writer,
                    "if iszero(slt(add(offset, 0x1f), end)) {{ {} }}",
                    failure_call
                );
                emitln!(ctx.writer, "let length := {}", array_length);
                emitln!(ctx.writer, "let size := {}", allocated_size);
                emitln!(
                    ctx.writer,
                    "array := {}({}, length, size, end)",
                    gen.generate_abi_decoding_array_available_len_memory(
                        ctx,
                        &ty,
                        &move_ty,
                        from_memory
                    ),
                    offset
                );
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate code to decode bytes
    fn generate_abi_decoding_bytes_available_len_memory(
        &mut self,
        ty: &SolidityType,
        from_memory: bool,
    ) -> String {
        let name_prefix = "abi_decode_available_length_";
        let from_memory_str = if from_memory { "_from_memory" } else { "" };
        let mut function_name = format!("{}_{}{}", name_prefix, ty, from_memory_str);
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(src, length, size, end) -> array ");
            let failure_call = gen.call_builtin_str(
                ctx,
                YulFunction::Abort,
                std::iter::once(ABI_DECODING_INVALID_BYTE_ARRAY_OFFSET.to_string()),
            );
            let allocation_size = gen.call_builtin_str(
                ctx,
                YulFunction::CheckMemorySize,
                std::iter::once("size".to_string()),
            );
            // Note: the code structure is different from the Solidity compiler code due to the representation of array in the memory
            ctx.emit_block(|| {
                let malloc = gen.call_builtin_str(
                    ctx,
                    YulFunction::Malloc,
                    std::iter::once(allocation_size.to_string()), // Make sure the allocated size is not over the limit
                );
                emitln!(ctx.writer, "array := {}", malloc);
                // store the current length of vector
                emitln!(
                    ctx.writer,
                    "{}",
                    gen.call_builtin_str(
                        ctx,
                        YulFunction::MemoryStoreU64,
                        vec!["array".to_string(), "length".to_string()].into_iter()
                    )
                );
                // store the capacity of vector
                let compute_capacity_str = gen.call_builtin_str(
                    ctx,
                    YulFunction::ClosestGreaterPowerOfTwo,
                    std::iter::once("length".to_string()),
                );
                emitln!(
                    ctx.writer,
                    "{}",
                    gen.call_builtin_str(
                        ctx,
                        YulFunction::MemoryStoreU64,
                        vec![
                            // TODO: simplify implementation of MemoryStoreU64?
                            "add(array, 8)".to_string(), // skip the length which is a u64 (8 bytes)
                            compute_capacity_str
                        ]
                        .into_iter()
                    )
                );
                emitln!(
                    ctx.writer,
                    "let dst := add(array, {})",
                    VECTOR_METADATA_SIZE // skip the metadata
                );
                emitln!(
                    ctx.writer,
                    "if gt(add(src, sub(size, {})), end) {{ {} }}", // prevent read beyond the size of calldata
                    VECTOR_METADATA_SIZE,
                    failure_call
                );
                let copy_fun = if from_memory {
                    YulFunction::CopyFromMemoryToMemory
                } else {
                    YulFunction::CopyFromCallDataToMemory
                };
                gen.call_builtin(
                    ctx,
                    copy_fun,
                    vec!["src".to_string(), "dst".to_string(), "length".to_string()].into_iter(),
                );
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate code to decode arrays
    fn generate_abi_decoding_array_available_len_memory(
        &mut self,
        ctx: &Context,
        ty: &SolidityType,
        move_ty: &Type,
        from_memory: bool,
    ) -> String {
        use SolidityType::*;

        if ty.is_bytes_type() {
            return self.generate_abi_decoding_bytes_available_len_memory(ty, from_memory);
        }

        let name_prefix = "abi_decode_available_length_";
        let from_memory_str = if from_memory { "_from_memory" } else { "" };
        let mut function_name = format!(
            "{}_{}_{}{}",
            name_prefix,
            ty,
            ctx.mangle_type(move_ty),
            from_memory_str
        );
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();
        let ty = ty.clone(); // need to move into lambda
        let move_ty = move_ty.clone();

        let inner_ty = match ty {
            DynamicArray(_ty) | StaticArray(_ty, _) => _ty,
            _ => panic!("wrong type"),
        };

        let inner_move_ty = match move_ty {
            Type::Vector(ref _ty) => _ty.clone(),
            _ => panic!("wrong type"),
        };

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(offset, length, size, end) -> array ");
            let failure_call = gen.call_builtin_str(
                ctx,
                YulFunction::Abort,
                std::iter::once(ABI_DECODING_INVALID_CALLDATA_ARRAY_OFFSET.to_string()),
            );
            let allocation_size = gen.call_builtin_str(
                ctx,
                YulFunction::CheckMemorySize,
                std::iter::once("size".to_string()),
            );
            let stride = &inner_ty.abi_head_size(true); // step of traversing the array in calldata
            let stride_dst = ctx.type_size(&inner_move_ty); // step of copying each element to memory
                                                            // Note: the code structure is different from the Solidity compiler code due to the representation of array in the memory
            ctx.emit_block(|| {
                let malloc = gen.call_builtin_str(
                    ctx,
                    YulFunction::Malloc,
                    std::iter::once(allocation_size.to_string()),
                );
                emitln!(ctx.writer, "array := {}", malloc);
                // current length of vector
                emitln!(
                    ctx.writer,
                    "{}",
                    gen.call_builtin_str(
                        ctx,
                        YulFunction::MemoryStoreU64,
                        vec!["array".to_string(), "length".to_string()].into_iter()
                    )
                );
                // capacity of vector
                let compute_capacity_str = gen.call_builtin_str(
                    ctx,
                    YulFunction::ClosestGreaterPowerOfTwo,
                    std::iter::once("length".to_string()),
                );
                emitln!(
                    ctx.writer,
                    "{}",
                    gen.call_builtin_str(
                        ctx,
                        YulFunction::MemoryStoreU64,
                        vec!["add(array, 8)".to_string(), compute_capacity_str].into_iter()
                    )
                );

                emitln!(
                    ctx.writer,
                    "let dst := add(array, {})",
                    VECTOR_METADATA_SIZE
                );
                emitln!(
                    ctx.writer,
                    "let srcEnd := add(offset, mul(length, {}))",
                    stride
                );
                emitln!(ctx.writer, "if gt(srcEnd, end) {{ {} }}", failure_call);

                emitln!(
                    ctx.writer,
                    "for {{ let src := offset }} lt(src, srcEnd) {{ src := add(src, {}) }}",
                    stride
                );
                ctx.emit_block(|| {
                    if !inner_ty.is_static() {
                        // if the inner type is dynamic, obtain the pointer to the array
                        let load = if from_memory { "mload" } else { "calldataload" };
                        emitln!(ctx.writer, "let innerOffset := {}(src)", load);
                        emitln!(
                            ctx.writer,
                            "if gt(innerOffset, 0xffffffffffffffff) {{ {} }}",
                            failure_call
                        );
                        emitln!(ctx.writer, "let elementPos := add(offset, innerOffset)");
                    } else {
                        emitln!(ctx.writer, "let elementPos := src");
                    }
                    emitln!(
                        ctx.writer,
                        "let value := {}(elementPos, end)",
                        gen.generate_abi_decoding_type(ctx, &inner_ty, &inner_move_ty, from_memory)
                    );
                    let memory_func = ctx.memory_store_builtin_fun(&inner_move_ty);
                    gen.call_builtin(
                        ctx,
                        memory_func,
                        vec!["dst".to_string(), "value".to_string()].into_iter(),
                    );
                    emitln!(ctx.writer, "dst := add(dst, {})", stride_dst);
                });
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate encoding functions for primitive types.
    fn generate_abi_encoding_primitive_type(
        &mut self,
        ty: &SolidityType,
        options: EncodingOptions,
    ) -> String {
        let name_prefix = "abi_encode";
        let function_name = format!("{}_{}", name_prefix, ty);
        let ty = ty.clone(); // need to move into lambda
        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value, pos) ");
            ctx.emit_block(|| {
                let mut store_str = format!("{}(value)", gen.generate_cleanup(&ty));
                if !options.padded {
                    store_str = format!("{}({})", gen.generate_left_align(&ty), store_str);
                }
                emitln!(ctx.writer, "mstore(pos, {})", store_str);
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate encoding functions for ty.
    fn generate_abi_encoding_type(
        &mut self,
        ctx: &Context,
        ty: &SolidityType,
        _loc: &SignatureDataLocation,
        move_ty: &Type,
        options: EncodingOptions,
    ) -> String {
        use SolidityType::*;
        match ty {
            Primitive(_) => self.generate_abi_encoding_primitive_type(ty, options),
            DynamicArray(_) | StaticArray(_, _) | Bytes | BytesStatic(_) | SolidityString => {
                self.generate_abi_encoding_array_type(ctx, ty, move_ty, options)
            }
            Struct(_, _) => self.generate_abi_encoding_struct_type(ctx, ty, move_ty, options),
            _ => "NYI".to_string(),
        }
    }

    /// Generate encoding functions for ty with updated position in calldata
    fn generate_abi_encoding_type_with_updated_pos(
        &mut self,
        ctx: &Context,
        ty: &SolidityType,
        _loc: &SignatureDataLocation,
        move_ty: &Type,
        options: EncodingOptions,
    ) -> String {
        use SolidityType::*;
        let ty = ty.clone();
        let move_ty = move_ty.clone();

        let encoding_function_name = match ty {
            Primitive(_) => self.generate_abi_encoding_primitive_type(&ty, options.clone()),
            DynamicArray(_) | StaticArray(_, _) | Bytes | BytesStatic(_) | SolidityString => {
                self.generate_abi_encoding_array_type(ctx, &ty, &move_ty, options.clone())
            }
            Struct(_, _) => {
                self.generate_abi_encoding_struct_type(ctx, &ty, &move_ty, options.clone())
            }
            _ => "NYI".to_string(),
        };
        let function_name = format!("{}_with_updated_pos", encoding_function_name);

        let data_size = ty.abi_head_size(options.padded);
        let generate_fun = move |_: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value, pos) -> updated_pos");
            let fun_str_call = format!("{}(value, pos)", encoding_function_name);
            if ty.is_static() {
                ctx.emit_block(|| {
                    emitln!(ctx.writer, "{}", fun_str_call);
                    emitln!(ctx.writer, "updated_pos := add(pos, {})", data_size);
                });
            } else {
                ctx.emit_block(|| {
                    emitln!(ctx.writer, "updated_pos := {}", fun_str_call);
                });
            }
        };

        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate encoding function for static and dynamic bytes and string
    fn generate_abi_encoding_bytes_memory(
        &mut self,
        ty: &SolidityType,
        options: EncodingOptions,
    ) -> String {
        let ty = ty.clone();
        assert!(ty.is_bytes_type(), "wrong type");
        let name_prefix = "abi_encode";
        let function_name = format!("{}_{}{}", name_prefix, ty, options.to_suffix());
        let return_value = (if ty.is_static() { "" } else { "-> end" }).to_string();
        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value, pos) {}", return_value);
            ctx.emit_block(|| {
                let size_fun = gen.call_builtin_str(
                    ctx,
                    YulFunction::MemoryLoadU64,
                    std::iter::once("value".to_string()),
                );
                let failure_size_match = gen.call_builtin_str(
                    ctx,
                    YulFunction::Abort,
                    std::iter::once(STATIC_ARRAY_SIZE_NOT_MATCH.to_string()),
                );
                emitln!(ctx.writer, "let size := {}", size_fun);
                if ty.is_static() {
                    if let SolidityType::BytesStatic(set_size) = ty {
                        emitln!(
                            ctx.writer,
                            "if iszero(eq(size, {})) {{ {} }}",
                            set_size,
                            failure_size_match
                        );
                    } else {
                        panic!("wrong types");
                    }
                }
                if !ty.is_array_static_size() && !options.in_place {
                    // for dynamic, write the length first
                    emitln!(ctx.writer, "mstore(pos, size)");
                    emitln!(ctx.writer, "pos := add(pos, 0x20)");
                }
                // compute the used memory space
                // copy the memory to pos
                gen.call_builtin(
                    ctx,
                    YulFunction::CopyMemory,
                    vec![
                        "add(value, 0x20)".to_string(),
                        "pos".to_string(),
                        "size".to_string(),
                    ]
                    .into_iter(),
                );
                if !ty.is_static() {
                    // bytes and string
                    if options.padded {
                        emitln!(
                            ctx.writer,
                            "size := {}",
                            gen.call_builtin_str(
                                ctx,
                                YulFunction::RoundUp,
                                std::iter::once("size".to_string())
                            )
                        );
                    }
                    emitln!(ctx.writer, "end := add(pos, size)");
                }
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate encoding function for array types including static and dynamic arrays, static and dynamic bytes and string
    fn generate_abi_encoding_array_type(
        &mut self,
        ctx: &Context,
        ty: &SolidityType,
        move_ty: &Type,
        options: EncodingOptions,
    ) -> String {
        use SolidityType::*;
        let sub_option = EncodingOptions {
            padded: true,
            in_place: options.in_place,
        };
        if ty.is_bytes_type() {
            return self.generate_abi_encoding_bytes_memory(ty, options);
        }
        let ty = ty.clone();
        let move_ty = move_ty.clone();
        let name_prefix = "abi_encode";
        let mut function_name = format!(
            "{}_{}_{}{}",
            name_prefix,
            ty,
            ctx.mangle_type(&move_ty),
            options.to_suffix()
        );
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            let ty_static = ty.is_static();
            let (inner_ty_static, inner_ty, set_size) = match ty {
                DynamicArray(ref _inner_ty) => (_inner_ty.is_static(), _inner_ty.clone(), 0),
                StaticArray(ref _inner_ty, _set_size) => {
                    (_inner_ty.is_static(), _inner_ty.clone(), _set_size)
                }
                _ => panic!("wrong type"),
            };
            let inner_move_ty = match move_ty {
                Type::Vector(ref _ty) => _ty.clone(),
                _ => panic!("wrong type"),
            };
            emit!(
                ctx.writer,
                "(value, pos) {} ",
                (if ty_static { "" } else { "-> end" }).to_string()
            );
            ctx.emit_block(|| {
                let size_fun = gen.call_builtin_str(
                    ctx,
                    YulFunction::MemoryLoadU64,
                    std::iter::once("value".to_string()),
                );
                let failure_size_match = gen.call_builtin_str(
                    ctx,
                    YulFunction::Abort,
                    std::iter::once(STATIC_ARRAY_SIZE_NOT_MATCH.to_string()),
                );
                // get length
                emitln!(ctx.writer, "let length := {}", size_fun);
                if ty.is_array_static_size() {
                    emitln!(
                        ctx.writer,
                        "if iszero(eq(length, {})) {{ {} }}",
                        set_size,
                        failure_size_match
                    );
                }
                if !ty.is_array_static_size() && !options.in_place {
                    // for dynamic, write the length first
                    emitln!(ctx.writer, "mstore(pos, length)");
                    emitln!(ctx.writer, "pos := add(pos, 0x20)");
                }
                if !inner_ty_static && !options.in_place {
                    // encode the pointer if the element is dynamic
                    emitln!(ctx.writer, "let headStart := pos");
                    emitln!(ctx.writer, "let tail := add(pos, mul(length, 0x20))");
                    // skip pointers
                }
                let stride = ctx.type_size(&inner_move_ty);
                emitln!(
                    ctx.writer,
                    "let start := add(value, {})",
                    VECTOR_METADATA_SIZE
                );
                emitln!(
                    ctx.writer,
                    "let srcEnd := add(start, mul(length, {}))",
                    stride
                );
                // copy the memory to offset
                emitln!(
                    ctx.writer,
                    "for {{ let src := start }} lt(src, srcEnd) {{ src := add(src, {}) }}",
                    stride
                );
                ctx.emit_block(|| {
                    if !inner_ty_static && !options.in_place {
                        emitln!(ctx.writer, "mstore(pos, sub(tail, headStart))");
                        // store the pointer
                    }
                    let memory_func = ctx.memory_load_builtin_fun(&inner_move_ty);
                    let load_fun =
                        gen.call_builtin_str(ctx, memory_func, std::iter::once("src".to_string()));
                    emitln!(ctx.writer, "let v := {}", load_fun); // load the value from array
                                                                  // call encoding function
                    if !inner_ty_static && !options.in_place {
                        // put the data to tail
                        emitln!(
                            ctx.writer,
                            "tail := {}(v, tail)",
                            gen.generate_abi_encoding_type_with_updated_pos(
                                ctx,
                                &inner_ty.clone(),
                                &SignatureDataLocation::Memory,
                                &inner_move_ty,
                                sub_option.clone()
                            )
                        );
                        emitln!(ctx.writer, "pos := add(pos, 0x20)");
                    } else {
                        // put the data in place
                        emitln!(
                            ctx.writer,
                            "pos := {}(v, pos)",
                            gen.generate_abi_encoding_type_with_updated_pos(
                                ctx,
                                &inner_ty.clone(),
                                &SignatureDataLocation::Memory,
                                &inner_move_ty,
                                sub_option.clone()
                            )
                        );
                    }
                });
                if !inner_ty_static && !options.in_place {
                    emitln!(ctx.writer, "pos := tail"); // new position moves to tail
                }
                if !ty_static {
                    emitln!(ctx.writer, "end := pos")
                }
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate encoding function for solidity structs
    fn generate_abi_encoding_struct_type(
        &mut self,
        ctx: &Context,
        ty: &SolidityType,
        move_ty: &Type,
        options: EncodingOptions,
    ) -> String {
        use SolidityType::*;
        assert!(matches!(ty, Struct(_, _)), "wrong types");
        let name_prefix = "abi_encode";
        let ty = ty.clone();

        let mut param_types = vec![];
        let mut move_tys = vec![];
        let mut real_offsets = vec![];
        if let Struct(_, ty_tuples) = ty.clone() {
            param_types = ty_tuples
                .iter()
                .map(|(_, _, _, _, ty)| ty.clone())
                .collect_vec();
            move_tys = ty_tuples
                .iter()
                .map(|(_, _, m_ty, _, _)| m_ty.clone())
                .collect_vec();
            real_offsets = ty_tuples
                .iter()
                .map(|(_, real_offset, _, _, _)| *real_offset)
                .collect_vec();
        }

        let mut function_name = format!(
            "{}_{}_{}{}",
            name_prefix,
            mangle_solidity_types(&param_types),
            ctx.mangle_types(&[move_ty.clone()]),
            options.to_suffix()
        );
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();

        let return_value = (if ty.is_static() { "" } else { "-> end" }).to_string();

        let sub_option = EncodingOptions {
            padded: true,
            in_place: options.in_place,
        };

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            let overall_type_head_vec = abi_head_sizes_vec(&param_types, true);
            let overall_type_head_size = abi_head_sizes_sum(&param_types, true);

            let ret_var = (0..overall_type_head_vec.len())
                .map(|i| format!("value_{}", i))
                .collect_vec();

            emit!(ctx.writer, "(value, pos) {} ", return_value);
            ctx.emit_block(|| {
                emitln!(
                    ctx.writer,
                    "let tail := add(pos, {})",
                    overall_type_head_size
                );

                assert!(real_offsets.len() == overall_type_head_vec.len());
                let mut head_pos = 0;
                for (stack_pos, (((ty, ty_size), move_ty), real_offset)) in overall_type_head_vec
                    .iter()
                    .zip(move_tys.iter())
                    .zip(real_offsets.iter())
                    .enumerate()
                {
                    let is_static = ty.is_static();
                    let local_typ_var = vec![ret_var[stack_pos].clone()];
                    let memory_func = ctx.memory_load_builtin_fun(move_ty);
                    if local_typ_var.len() == 1 {
                        emitln!(
                            ctx.writer,
                            "let {} := {}",
                            local_typ_var[0].clone(),
                            gen.call_builtin_str(
                                ctx,
                                memory_func,
                                std::iter::once(format!("add(value, {})", real_offset))
                            )
                        );
                        if options.in_place {
                            emitln!(
                                ctx.writer,
                                "pos := {}({}, pos)",
                                gen.generate_abi_encoding_type_with_updated_pos(
                                    ctx,
                                    &ty.clone(),
                                    &SignatureDataLocation::Memory,
                                    move_ty,
                                    sub_option.clone()
                                ),
                                local_typ_var[0].clone()
                            );
                        } else {
                            if is_static {
                                emitln!(
                                    ctx.writer,
                                    "{}({}, add(pos, {}))",
                                    gen.generate_abi_encoding_type(
                                        ctx,
                                        &ty.clone(),
                                        &SignatureDataLocation::Memory,
                                        move_ty,
                                        sub_option.clone()
                                    ),
                                    local_typ_var[0].clone(),
                                    head_pos
                                );
                            } else {
                                emitln!(
                                    ctx.writer,
                                    "mstore(add(pos, {}), sub(tail, pos))",
                                    head_pos
                                );
                                emitln!(
                                    ctx.writer,
                                    "tail := {}({}, tail)",
                                    gen.generate_abi_encoding_type(
                                        ctx,
                                        &ty.clone(),
                                        &SignatureDataLocation::Memory,
                                        move_ty,
                                        sub_option.clone()
                                    ),
                                    local_typ_var[0].clone()
                                );
                            }
                            head_pos += ty_size;
                        }
                    }
                }
                if !ty.is_static() && options.in_place {
                    emitln!(ctx.writer, "end := pos");
                } else if !ty.is_static() && !options.in_place {
                    emitln!(ctx.writer, "end := tail");
                }
            })
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    pub(crate) fn generate_abi_tuple_encoding(
        &mut self,
        ctx: &Context,
        param_types: Vec<SolidityType>,
        param_locs: Vec<SignatureDataLocation>,
        move_tys: Vec<Type>,
    ) -> String {
        let options = EncodingOptions {
            padded: true,
            in_place: false,
        };
        let name_prefix = "abi_encode_tuple";
        let mut function_name = format!(
            "{}_{}_{}{}",
            name_prefix,
            mangle_solidity_types(&param_types),
            ctx.mangle_types(&move_tys),
            options.to_suffix()
        );
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            let mut value_params = (0..param_types.len())
                .map(|i| format!("value_{}", i))
                .join(", ");
            if !value_params.is_empty() {
                value_params = format!(",{}", value_params);
            }
            emit!(ctx.writer, "(headStart {}) -> tail ", value_params);
            ctx.emit_block(|| {
                let overall_type_head_vec = abi_head_sizes_vec(&param_types, options.padded);
                let overall_type_head_size = abi_head_sizes_sum(&param_types, options.padded);
                emitln!(
                    ctx.writer,
                    "tail := add(headStart, {})",
                    overall_type_head_size
                );
                let mut head_pos = 0;
                for (stack_pos, (((ty, ty_size), _loc), move_ty)) in overall_type_head_vec
                    .iter()
                    .zip(param_locs.iter())
                    .zip(move_tys.iter())
                    .enumerate()
                {
                    let is_static = ty.is_static();
                    let mut local_typ_var = vec![];
                    // TODO: consider the case size_on_stack is not 1
                    local_typ_var.push(format!("value_{}", stack_pos));
                    let mut values = local_typ_var.iter().join(", ");
                    let abi_encode_type =
                        gen.generate_abi_encoding_type(ctx, ty, _loc, move_ty, options.clone());
                    if is_static {
                        emitln!(
                            ctx.writer,
                            "{}({}, add(headStart, {}))",
                            abi_encode_type,
                            values,
                            head_pos
                        );
                    } else {
                        // TODO: dynamic types need to be revisited
                        emitln!(
                            ctx.writer,
                            "mstore(add(headStart, {}), sub(tail, headStart))",
                            head_pos
                        );
                        if !values.is_empty() {
                            values = format!("{}, ", values);
                        }
                        emitln!(ctx.writer, "tail := {}({} tail)", abi_encode_type, values);
                    }
                    head_pos += ty_size;
                }
            })
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate encoding functions for tuple.
    pub(crate) fn generate_abi_tuple_encoding_ret(
        &mut self,
        ctx: &Context,
        sig: &SoliditySignature,
        move_tys: Vec<Type>,
    ) -> String {
        let param_types = sig.ret_types.iter().map(|(ty, _)| ty.clone()).collect_vec(); // need to move into lambda
        let param_locs = sig
            .ret_types
            .iter()
            .map(|(_, loc)| loc.clone())
            .collect_vec();
        self.generate_abi_tuple_encoding(ctx, param_types, param_locs, move_tys)
    }

    /// Generate encoding functions for tuple in parameters.
    pub(crate) fn generate_abi_tuple_encoding_para(
        &mut self,
        ctx: &Context,
        sig: &SoliditySignature,
        move_tys: Vec<Type>,
        packed_flag: bool,
    ) -> String {
        let param_types = sig
            .para_types
            .iter()
            .map(|(ty, _, _)| ty.clone())
            .collect_vec(); // need to move into lambda
        let param_locs = sig
            .para_types
            .iter()
            .map(|(_, _, loc)| loc.clone())
            .collect_vec();
        if packed_flag {
            self.generate_abi_tuple_encoding_packed(ctx, param_types, param_locs, move_tys)
        } else {
            self.generate_abi_tuple_encoding(ctx, param_types, param_locs, move_tys)
        }
    }

    /// Generate encodePacked function
    pub(crate) fn generate_abi_tuple_encoding_packed(
        &mut self,
        ctx: &Context,
        param_types: Vec<SolidityType>,
        param_locs: Vec<SignatureDataLocation>,
        move_tys: Vec<Type>,
    ) -> String {
        let options = EncodingOptions {
            padded: false,
            in_place: true,
        };
        let name_prefix = "abi_encode_tuple_packed";
        let mut function_name = format!(
            "{}_{}_{}{}",
            name_prefix,
            mangle_solidity_types(&param_types),
            ctx.mangle_types(&move_tys),
            options.to_suffix()
        );
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            let mut value_params = (0..param_types.len())
                .map(|i| format!("value_{}", i))
                .join(", ");
            if !value_params.is_empty() {
                value_params = format!(",{}", value_params);
            }
            emit!(ctx.writer, "(pos {}) -> end ", value_params);
            ctx.emit_block(|| {
                let overall_type_head_vec = abi_head_sizes_vec(&param_types, options.padded);
                for (stack_pos, (((ty, ty_size), _loc), move_ty)) in overall_type_head_vec
                    .iter()
                    .zip(param_locs.iter())
                    .zip(move_tys.iter())
                    .enumerate()
                {
                    let is_static = ty.is_static();
                    let mut local_typ_var = vec![];
                    // TODO: consider the case size_on_stack is not 1
                    local_typ_var.push(format!("value_{}", stack_pos));
                    let mut values = local_typ_var.iter().join(", ");
                    if !values.is_empty() {
                        values = format!("{}, ", values);
                    }
                    let abi_encode_type =
                        gen.generate_abi_encoding_type(ctx, ty, _loc, move_ty, options.clone());
                    if !is_static {
                        // dynamic
                        emitln!(ctx.writer, "pos := {}({} pos)", abi_encode_type, values);
                    } else {
                        // static
                        emitln!(ctx.writer, "{}({} pos)", abi_encode_type, values);
                        emitln!(ctx.writer, "pos := add(pos, {})", ty_size);
                    }
                }
                emitln!(ctx.writer, "end := pos");
            })
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    /// Generate encodePacked function with keccak256 hashing
    pub(crate) fn generate_packed_hashed(
        &mut self,
        ctx: &Context,
        tys: Vec<SolidityType>,
        param_locs: Vec<SignatureDataLocation>,
        move_tys: Vec<Type>,
    ) -> String {
        let name_prefix = "packed_hashed_";
        let mut function_name = format!(
            "{}_{}_{}",
            name_prefix,
            mangle_solidity_types(&tys),
            ctx.mangle_types(&move_tys)
        );
        let re = Regex::new(r"[()\[\],]").unwrap();
        function_name = re.replace_all(&function_name, "_").to_string();

        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            let mut value_params = (0..tys.len()).map(|i| format!("value_{}", i)).join(", ");
            emit!(ctx.writer, "({}) -> hash ", value_params);
            if !value_params.is_empty() {
                value_params = format!(",{}", value_params);
            }
            let generate_encoding_packed =
                gen.generate_abi_tuple_encoding_packed(ctx, tys, param_locs, move_tys);
            ctx.emit_block(|| {
                emitln!(
                    ctx.writer,
                    "let pos := mload({})",
                    substitute_placeholders("${MEM_SIZE_LOC}").unwrap()
                );
                emitln!(
                    ctx.writer,
                    "let end := {}(pos {})",
                    generate_encoding_packed,
                    value_params
                );
                emitln!(
                    ctx.writer,
                    "mstore({}, end)",
                    substitute_placeholders("${MEM_SIZE_LOC}").unwrap(),
                );
                emitln!(ctx.writer, "hash := keccak256(pos, sub(end, pos))");
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }
}
