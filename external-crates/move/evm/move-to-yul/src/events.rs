// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    functions::FunctionGenerator,
    solidity_ty::{SignatureDataLocation, SolidityType, PARSE_ERR_MSG},
    yul_functions::substitute_placeholders,
};
use std::{fmt, fmt::Formatter};

use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;

use anyhow::{anyhow, Context as AnyhowContext};
use move_model::{
    emit, emitln,
    model::{FunId, QualifiedInstId, StructEnv, StructId},
    ty::Type,
};
use move_stackless_bytecode::function_target_pipeline::FunctionVariant;
use sha3::{Digest, Keccak256, Sha3_256};

pub(crate) const COMPATIBILITY_ERROR: &str =
    "event signature is not compatible with the move struct";
pub(crate) const TOPIC_COUNT_ERROR: &str = "too many indexed arguments";

/// Represents an event Signature appearing in the event attribute.
#[derive(Debug, Clone)]
pub(crate) struct EventSignature {
    pub event_name: String,
    pub para_types: Vec<(usize, SolidityType, Type, bool, String)>,
    pub indexed_count: usize,
}

// ================================================================================================
// Pretty print for EventSignature

impl fmt::Display for EventSignature {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.selector_signature())
    }
}

impl EventSignature {
    fn get_ordered_types(st: &StructEnv<'_>) -> std::vec::IntoIter<(usize, Type, String)> {
        st.get_fields()
            .map(|field| {
                let field_type = field.get_type();
                let field_name = st.symbol_pool().string(field.get_name()).to_string();
                (field.get_offset(), field_type, field_name)
            })
            .sorted_by_key(|(offset, _, _)| *offset)
    }

    /// Create a default event signature from a move struct definition
    pub fn create_default_event_signature(ctx: &Context, st: &StructEnv<'_>) -> Self {
        let st_name = st.symbol_pool().string(st.get_name()).to_string();
        let mut para_type_lst = vec![];

        let ordered_types = EventSignature::get_ordered_types(st);
        for (offset, move_ty, field_name) in ordered_types
            .map(|(offset, ty, field_name)| (offset, ty, field_name))
            .collect_vec()
        {
            let solidity_ty = SolidityType::translate_from_move(ctx, &move_ty, false); // implicit mapping from a move type to a solidity type
            para_type_lst.push((offset, solidity_ty, move_ty.clone(), false, field_name));
            // no index by default
        }
        EventSignature {
            event_name: st_name,
            para_types: para_type_lst,
            indexed_count: 1,
        }
    }

    /// Generate parameter list for computing the function selector
    fn compute_param_types(&self, param_types: &[&SolidityType]) -> String {
        let display_type_slice = |tys: &[&SolidityType]| -> String {
            tys.iter()
                .map(|t| format!("{}", t))
                .collect::<Vec<_>>()
                .join(",")
        };
        display_type_slice(param_types)
    }

    fn selector_signature(&self) -> String {
        format!(
            "{}({})",
            self.event_name,
            self.compute_param_types(
                &self
                    .para_types
                    .iter()
                    .map(|(_, ty, _, _, _)| ty)
                    .collect_vec()
            )
        )
    }

    /// Parse the event signature
    pub fn parse_into_event_signature(
        ctx: &Context,
        sig_str: &str,
        st: &StructEnv<'_>,
    ) -> anyhow::Result<Self> {
        // Event signature matching
        static SIG_REG: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s*(?P<sig_name>[a-zA-Z_$][a-zA-Z_$0-9]*)\s*\((?P<args>[^)]*)\)$")
                .unwrap()
        });
        if let Some(parsed) = SIG_REG.captures(sig_str.trim()) {
            let sig_name = parsed.name("sig_name").context(PARSE_ERR_MSG)?.as_str();
            let para_type_str = parsed.name("args").context(PARSE_ERR_MSG)?.as_str();
            let (para_types, indexed_count) =
                EventSignature::extract_para_type_str(ctx, para_type_str)?;

            // Number of topics cannot be greater than 4
            if indexed_count > 4 {
                return Err(anyhow!(TOPIC_COUNT_ERROR));
            }

            // Check parameter type list
            let ordered_types = EventSignature::get_ordered_types(st);
            let sig_para_vec = para_types.iter().map(|(ty, _)| ty).collect::<Vec<_>>();
            if sig_para_vec.len() != ordered_types.len() {
                return Err(anyhow!(COMPATIBILITY_ERROR));
            }
            let mut offset_para_types = vec![];
            for ((off, m_ty, field_name), (s_ty, b)) in ordered_types.zip(para_types.into_iter()) {
                if !s_ty.check_type_compatibility(ctx, &m_ty) {
                    return Err(anyhow!(COMPATIBILITY_ERROR));
                }
                offset_para_types.push((off, s_ty.clone(), m_ty.clone(), b, field_name));
            }

            let event_sig = EventSignature {
                event_name: sig_name.to_string(),
                para_types: offset_para_types,
                indexed_count,
            };
            Ok(event_sig)
        } else {
            Err(anyhow!(PARSE_ERR_MSG))
        }
    }

    /// Generate pairs of solidity type and location and the number of indexed parameters
    fn extract_para_type_str(
        ctx: &Context,
        args: &str,
    ) -> anyhow::Result<(Vec<(SolidityType, bool)>, usize)> {
        let args_trim = args.trim();
        let mut indexed_count = 1;
        if args_trim.is_empty() {
            return Ok((vec![], indexed_count));
        }
        let mut ret_vec = vec![];
        let paras = args_trim.split(',').collect_vec();
        for para in paras {
            let para_trim = para.trim();
            if para_trim.is_empty() {
                return Err(anyhow!(PARSE_ERR_MSG));
            }
            let mut index_flag = false;
            let mut para_type_str = para_trim;
            if let Some(stripped_indexed) = para_trim.strip_suffix("indexed") {
                let stripped_trimmed = stripped_indexed.trim();
                if stripped_trimmed.ends_with(']')
                    || stripped_trimmed.len() < stripped_indexed.len()
                {
                    index_flag = true;
                    indexed_count += 1;
                    para_type_str = stripped_trimmed;
                } else {
                    return Err(anyhow!(PARSE_ERR_MSG));
                }
            }
            let ty = SolidityType::parse(ctx, para_type_str)?;
            ret_vec.push((ty, index_flag));
        }
        Ok((ret_vec, indexed_count))
    }
}

/// Generate event emit functions for send_<name> in async move contracts
/// example: the function send_foo(actor: address, args) will emit an ethereum event
/// Foo(actor, message_hash, args) where encoding follows the ethereum standard
/// TODO: add a sequence number to the event to help perform deduplication on the listener side
pub(crate) fn define_emit_fun_for_send(
    gen: &mut FunctionGenerator,
    ctx: &Context,
    event_sig: &EventSignature,
    fun_id: &QualifiedInstId<FunId>,
) {
    let fun = ctx.env.get_function(fun_id.to_qualified_id());
    let target = &ctx.targets.get_target(&fun, &FunctionVariant::Baseline);

    // Emit function header
    let params = (0..target.get_parameter_count()).map(|idx| ctx.make_local_name(target, idx));
    let params_str = params.clone().join(",");
    let params_vec = params.collect_vec();

    let mut local_name_idx = target.get_parameter_count();

    emit!(
        ctx.writer,
        "function {}({}) ",
        ctx.make_function_name(fun_id),
        params_str
    );

    // Generate the function body
    ctx.emit_block(|| {
        let signature_types = &event_sig.para_types;
        let topic_0_var = ctx.make_local_name(target, local_name_idx);
        local_name_idx += 1;
        let event_sig_str = event_sig.to_string();
        let topic_0_hash = format!("0x{:x}", Keccak256::digest(event_sig_str.as_bytes()));

        emitln!(ctx.writer, "let {} := {}", topic_0_var, topic_0_hash);

        // Compute the message hash following the logic in move_compiler::attr_derivation::async_deriver::message_hash
        // message_hash is the first 8 bytes of Sha3_256::digest(address::module_name::foo)
        // example: 0x00000000000000000000000000000003::AccountStateMachine::deposit
        let message_str = format!(
            "0x{}::{}",
            fun.module_env.self_address(),
            fun.get_full_name_str().replace("send_", "")
        );
        let hash_bytes = Sha3_256::digest(message_str.as_bytes());
        let message_hash_str = &format!("0x{:x}", hash_bytes)[0..10];
        let message_hash_var = ctx.make_local_name(target, local_name_idx);
        local_name_idx += 1;
        emitln!(
            ctx.writer,
            "let {} := {}",
            message_hash_var,
            message_hash_str
        );

        let mut indexed_paras = vec![];
        let mut indexed_vars = vec![topic_0_var];
        let mut unindexed_paras = vec![];
        let mut unindexed_vars = vec![];

        // For async move, there is no index for any parameters
        // TODO: consider index receipient address and hash_messages by default
        // TODO: consider add a sig to the attribute #[message] to specify index other parameters, e.g.
        // #message[sig=b"xfer(address indexed, address indexed, uint128 indexed)]
        for (i, (_, solidity_ty, move_ty, indexed_flag, _)) in signature_types.iter().enumerate() {
            let mut var = if i == 0 {
                params_vec.get(0).unwrap().to_string()
            } else if i == 1 {
                message_hash_var.clone()
            } else {
                params_vec.get(i - 1).unwrap().to_string()
            };

            if *indexed_flag {
                indexed_paras.push((solidity_ty.clone(), move_ty.clone()));
                if !solidity_ty.is_value_type() {
                    // for non-value type
                    let new_var = ctx.make_local_name(target, local_name_idx);
                    local_name_idx += 1;
                    emitln!(
                        ctx.writer,
                        "let {} := {}({})",
                        new_var,
                        gen.parent.generate_packed_hashed(
                            ctx,
                            vec![solidity_ty.clone()],
                            vec![SignatureDataLocation::Memory],
                            vec![move_ty.clone()]
                        ),
                        var
                    );
                    var = new_var;
                }
                indexed_vars.push(var.clone());
            } else {
                unindexed_paras.push((solidity_ty.clone(), move_ty.clone()));
                unindexed_vars.push(var.clone());
            }
        }

        ctx.emit_block(|| {
            let pos_var = ctx.make_local_name(target, local_name_idx);
            local_name_idx += 1;
            let end_var = ctx.make_local_name(target, local_name_idx);
            local_name_idx += 1;
            emitln!(
                ctx.writer,
                "let {} := mload({})",
                pos_var,
                substitute_placeholders("${MEM_SIZE_LOC}").unwrap()
            );
            // Create dummy signature location
            let sig_para_locs = vec![SignatureDataLocation::Memory; unindexed_paras.len()];
            let para_types = unindexed_paras
                .iter()
                .map(|(_, move_ty)| move_ty.clone())
                .collect_vec();
            let sig_para_vec = unindexed_paras
                .iter()
                .map(|(solidity_ty, _)| solidity_ty.clone())
                .collect_vec();
            // Generate encoding function for unindexed parameters
            let encode_unindexed = gen.parent.generate_abi_tuple_encoding(
                ctx,
                sig_para_vec,
                sig_para_locs,
                para_types,
            );
            let unindexed_str = if !unindexed_vars.is_empty() {
                format!(", {}", unindexed_vars.join(","))
            } else {
                "".to_string()
            };
            emitln!(
                ctx.writer,
                "let {} := {}({}{})",
                end_var,
                encode_unindexed,
                pos_var,
                unindexed_str
            );
            // Generate the code to call log opcode
            let indexed_str = if !indexed_vars.is_empty() {
                format!(", {}", indexed_vars.join(","))
            } else {
                "".to_string()
            };
            emitln!(
                ctx.writer,
                "log{}({}, sub({}, {}){})",
                event_sig.indexed_count,
                pos_var,
                end_var,
                pos_var,
                indexed_str
            );
            emitln!(
                ctx.writer,
                "mstore({}, {})",
                substitute_placeholders("${MEM_SIZE_LOC}").unwrap(),
                end_var
            );
        });
    });
}

/// Generate emit functions
pub(crate) fn define_emit_fun(
    gen: &mut FunctionGenerator,
    ctx: &Context,
    event_sig: &EventSignature,
    st_id: &QualifiedInstId<StructId>,
    fun_id: &QualifiedInstId<FunId>,
) {
    let fun = ctx.env.get_function(fun_id.to_qualified_id());
    let target = &ctx.targets.get_target(&fun, &FunctionVariant::Baseline);
    assert!(
        target.get_parameter_count() == 1,
        "parameter number must be 1"
    );

    // Emit function header
    let param_name = ctx.make_local_name(target, 0);
    let mut local_name_idx = 1;
    emit!(
        ctx.writer,
        "function {}({}) ",
        ctx.make_function_name(fun_id),
        param_name
    );

    // Obtain the layout of the struct
    let layout = ctx.get_struct_layout(st_id);

    // Generate the function body
    ctx.emit_block(|| {
        let signature_types = &event_sig.para_types;
        let topic_0_var = ctx.make_local_name(target, local_name_idx);
        local_name_idx += 1;
        let event_sig_str = event_sig.to_string();
        let topic_0_hash = format!("0x{:x}", Keccak256::digest(event_sig_str.as_bytes()));

        emitln!(ctx.writer, "let {} := {}", topic_0_var, topic_0_hash);
        let mut indexed_paras = vec![];
        let mut indexed_vars = vec![topic_0_var];
        let mut unindexed_paras = vec![];
        let mut unindexed_vars = vec![];
        for (offset, solidity_ty, move_ty, indexed_flag, _) in signature_types {
            let (real_offset, _) = layout.offsets.get(offset).unwrap();
            let mut var = ctx.make_local_name(target, local_name_idx);
            local_name_idx += 1;
            let memory_func = ctx.memory_load_builtin_fun(move_ty);
            let load_fun = gen.parent.call_builtin_str(
                ctx,
                memory_func,
                std::iter::once(format!("add({}, {})", param_name, real_offset)),
            );
            emitln!(ctx.writer, "let {} := {}", var, load_fun);
            if *indexed_flag {
                indexed_paras.push((solidity_ty.clone(), move_ty.clone()));
                if !solidity_ty.is_value_type() {
                    // for non-value type
                    let new_var = ctx.make_local_name(target, local_name_idx);
                    local_name_idx += 1;
                    emitln!(
                        ctx.writer,
                        "let {} := {}({})",
                        new_var,
                        gen.parent.generate_packed_hashed(
                            ctx,
                            vec![solidity_ty.clone()],
                            vec![SignatureDataLocation::Memory],
                            vec![move_ty.clone()]
                        ),
                        var
                    );
                    var = new_var;
                }
                indexed_vars.push(var.clone());
            } else {
                unindexed_paras.push((solidity_ty.clone(), move_ty.clone()));
                unindexed_vars.push(var.clone());
            }
        }

        ctx.emit_block(|| {
            let pos_var = ctx.make_local_name(target, local_name_idx);
            local_name_idx += 1;
            let end_var = ctx.make_local_name(target, local_name_idx);
            local_name_idx += 1;
            emitln!(
                ctx.writer,
                "let {} := mload({})",
                pos_var,
                substitute_placeholders("${MEM_SIZE_LOC}").unwrap()
            );
            // Create dummy signature location
            let sig_para_locs = vec![SignatureDataLocation::Memory; unindexed_paras.len()];
            let para_types = unindexed_paras
                .iter()
                .map(|(_, move_ty)| move_ty.clone())
                .collect_vec();
            let sig_para_vec = unindexed_paras
                .iter()
                .map(|(solidity_ty, _)| solidity_ty.clone())
                .collect_vec();
            // Generate encoding function for unindexed parameters
            let encode_unindexed = gen.parent.generate_abi_tuple_encoding(
                ctx,
                sig_para_vec,
                sig_para_locs,
                para_types,
            );
            let unindexed_str = if !unindexed_vars.is_empty() {
                format!(", {}", unindexed_vars.join(","))
            } else {
                "".to_string()
            };
            emitln!(
                ctx.writer,
                "let {} := {}({}{})",
                end_var,
                encode_unindexed,
                pos_var,
                unindexed_str
            );
            // Generate the code to call log opcode
            let indexed_str = if !indexed_vars.is_empty() {
                format!(", {}", indexed_vars.join(","))
            } else {
                "".to_string()
            };
            emitln!(
                ctx.writer,
                "log{}({}, sub({}, {}){})",
                event_sig.indexed_count,
                pos_var,
                end_var,
                pos_var,
                indexed_str
            );
            emitln!(
                ctx.writer,
                "mstore({}, {})",
                substitute_placeholders("${MEM_SIZE_LOC}").unwrap(),
                end_var
            );
        });
    });
}
