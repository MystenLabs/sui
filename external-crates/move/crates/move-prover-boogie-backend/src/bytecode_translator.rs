// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module translates the bytecode of a module to Boogie code.

use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use codespan::LineIndex;
use itertools::Itertools;
#[allow(unused_imports)]
use log::{debug, info, log, warn, Level};

use move_binary_format::normalized::Function;
use move_compiler::{hlir::ast::Var, interface_generator::NATIVE_INTERFACE};
use move_core_types::language_storage::StructTag;
use move_model::{
    ast::{Attribute, TempIndex, TraceKind},
    code_writer::CodeWriter,
    emit, emitln,
    model::{
        DatatypeId, FieldId, FunId, FunctionEnv, GlobalEnv, Loc, NodeId, QualifiedId,
        QualifiedInstId, StructEnv,
    },
    pragmas::{ADDITION_OVERFLOW_UNCHECKED_PRAGMA, SEED_PRAGMA, TIMEOUT_PRAGMA},
    ty::{PrimitiveType, Type, TypeDisplayContext, BOOL_TYPE},
    well_known::{TYPE_INFO_MOVE, TYPE_NAME_GET_MOVE, TYPE_NAME_MOVE},
};
use move_stackless_bytecode::{
    function_data_builder::FunctionDataBuilder,
    function_target::FunctionTarget,
    function_target_pipeline::{
        FunctionTargetProcessor, FunctionTargetsHolder, FunctionVariant, VerificationFlavor,
    },
    livevar_analysis::LiveVarAnalysisProcessor,
    mono_analysis::{self, MonoInfo},
    number_operation::{
        FuncOperationMap, GlobalNumberOperationState,
        NumOperation::{self, Bitwise, Bottom},
    },
    options::ProverOptions,
    reaching_def_analysis::ReachingDefProcessor,
    spec_global_variable_analysis::{self},
    stackless_bytecode::{
        AbortAction, BorrowEdge, BorrowNode, Bytecode, Constant, HavocKind, IndexEdgeKind,
        Operation, PropKind,
    },
    verification_analysis,
};

use crate::{
    boogie_helpers::{
        boogie_address_blob, boogie_bv_type, boogie_byte_blob, boogie_constant_blob,
        boogie_debug_track_abort, boogie_debug_track_local, boogie_debug_track_return,
        boogie_declare_global, boogie_equality_for_type, boogie_field_sel, boogie_field_update,
        boogie_function_bv_name, boogie_function_name, boogie_make_vec_from_strings,
        boogie_modifies_memory_name, boogie_num_literal, boogie_num_type_base,
        boogie_num_type_string_capital, boogie_reflection_type_info, boogie_reflection_type_name,
        boogie_resource_memory_name, boogie_spec_global_var_name, boogie_struct_name, boogie_temp,
        boogie_temp_from_suffix, boogie_type, boogie_type_param, boogie_type_suffix,
        boogie_type_suffix_bv, boogie_type_suffix_for_struct, boogie_well_formed_check,
        boogie_well_formed_expr_bv, FunctionTranslationStyle, TypeIdentToken,
    },
    options::BoogieOptions,
    spec_translator::SpecTranslator,
};

pub struct BoogieTranslator<'env> {
    env: &'env GlobalEnv,
    options: &'env BoogieOptions,
    writer: &'env CodeWriter,
    spec_translator: SpecTranslator<'env>,
    targets: &'env FunctionTargetsHolder,
}

pub struct FunctionTranslator<'env> {
    parent: &'env BoogieTranslator<'env>,
    fun_target: &'env FunctionTarget<'env>,
    type_inst: &'env [Type],
    style: FunctionTranslationStyle,
}

pub struct StructTranslator<'env> {
    parent: &'env BoogieTranslator<'env>,
    struct_env: &'env StructEnv<'env>,
    type_inst: &'env [Type],
}

impl<'env> BoogieTranslator<'env> {
    pub fn new(
        env: &'env GlobalEnv,
        options: &'env BoogieOptions,
        targets: &'env FunctionTargetsHolder,
        writer: &'env CodeWriter,
    ) -> Self {
        Self {
            env,
            options,
            targets,
            writer,
            spec_translator: SpecTranslator::new(writer, env, options),
        }
    }

    pub fn translate(&mut self) {
        let writer = self.writer;
        let env = self.env;

        let mono_info = mono_analysis::get_info(self.env);
        let empty = &BTreeSet::new();

        emitln!(
            writer,
            "\n\n//==================================\n// Begin Translation\n"
        );

        // Add type reflection axioms
        if !mono_info.type_params.is_empty() {
            emitln!(writer, "function $TypeName(t: $TypeParamInfo): Vec int;");

            // type name <-> type info: primitives
            for name in [
                "Bool", "U8", "U16", "U32", "U64", "U128", "U256", "Address", "Signer",
            ]
            .into_iter()
            {
                emitln!(
                    writer,
                    "axiom (forall t: $TypeParamInfo :: {{$TypeName(t)}} \
                            t is $TypeParam{} ==> $IsEqual'vec'u8''($TypeName(t), {}));",
                    name,
                    TypeIdentToken::convert_to_bytes(TypeIdentToken::make(&name.to_lowercase()))
                );
                emitln!(
                    writer,
                    "axiom (forall t: $TypeParamInfo :: {{$TypeName(t)}} \
                            $IsEqual'vec'u8''($TypeName(t), {}) ==> t is $TypeParam{});",
                    TypeIdentToken::convert_to_bytes(TypeIdentToken::make(&name.to_lowercase())),
                    name,
                );
            }

            // type name <-> type info: vector
            let mut tokens = TypeIdentToken::make("vector<");
            tokens.push(TypeIdentToken::Variable("$TypeName(t->e)".to_string()));
            tokens.extend(TypeIdentToken::make(">"));
            emitln!(
                writer,
                "axiom (forall t: $TypeParamInfo :: {{$TypeName(t)}} \
                            t is $TypeParamVector ==> $IsEqual'vec'u8''($TypeName(t), {}));",
                TypeIdentToken::convert_to_bytes(tokens)
            );
            // TODO(mengxu): this will parse it to an uninterpreted vector element type
            emitln!(
                writer,
                "axiom (forall t: $TypeParamInfo :: {{$TypeName(t)}} \
                            ($IsPrefix'vec'u8''($TypeName(t), {}) && $IsSuffix'vec'u8''($TypeName(t), {})) ==> t is $TypeParamVector);",
                TypeIdentToken::convert_to_bytes(TypeIdentToken::make("vector<")),
                TypeIdentToken::convert_to_bytes(TypeIdentToken::make(">")),
            );

            // type name <-> type info: struct
            let mut tokens = TypeIdentToken::make("0x");
            // TODO(mengxu): this is not a correct radix16 encoding of an integer
            tokens.push(TypeIdentToken::Variable("MakeVec1(t->a)".to_string()));
            tokens.extend(TypeIdentToken::make("::"));
            tokens.push(TypeIdentToken::Variable("t->m".to_string()));
            tokens.extend(TypeIdentToken::make("::"));
            tokens.push(TypeIdentToken::Variable("t->s".to_string()));
            emitln!(
                writer,
                "axiom (forall t: $TypeParamInfo :: {{$TypeName(t)}} \
                            t is $TypeParamStruct ==> $IsEqual'vec'u8''($TypeName(t), {}));",
                TypeIdentToken::convert_to_bytes(tokens)
            );
            // TODO(mengxu): this will parse it to an uninterpreted struct
            emitln!(
                writer,
                "axiom (forall t: $TypeParamInfo :: {{$TypeName(t)}} \
                            $IsPrefix'vec'u8''($TypeName(t), {}) ==> t is $TypeParamVector);",
                TypeIdentToken::convert_to_bytes(TypeIdentToken::make("0x")),
            );
        }

        // Add given type declarations for type parameters.
        emitln!(writer, "\n\n// Given Types for Type Parameters\n");
        for idx in &mono_info.type_params {
            let param_type = boogie_type_param(env, *idx);
            let suffix = boogie_type_suffix(env, &Type::TypeParameter(*idx));
            let is_uid = self
                .env
                .find_struct_by_tag(&StructTag::from_str("0x2::object::UID").unwrap())
                .is_some();
            if is_uid {
                // Sui-specific to allow "using" unresolved type params as Sui objects in Boogie
                // (otherwise Boogie compilation errors may occur)
                emitln!(writer, "datatype {} {{", param_type);
                emitln!(writer, "    {}($id: $2_object_UID)", param_type);
                emitln!(writer, "}");
            } else {
                emitln!(writer, "type {};", param_type);
            }
            emitln!(
                writer,
                "function {{:inline}} $IsEqual'{}'(x1: {}, x2: {}): bool {{ x1 == x2 }}",
                suffix,
                param_type,
                param_type
            );
            emitln!(
                writer,
                "function {{:inline}} $IsValid'{}'(x: {}): bool {{ true }}",
                suffix,
                param_type,
            );
            emitln!(
                writer,
                "procedure {{:inline 1}} $0_prover_type_inv'{}'(x: {}) returns (res: bool) {{ res := true; }}",
                suffix,
                param_type,
            );

            // declare free variables to represent the type info for this type
            emitln!(writer, "var {}_info: $TypeParamInfo;", param_type);
        }
        emitln!(writer);

        self.spec_translator
            .translate_axioms(env, mono_info.as_ref());

        self.translate_ghost_global(&mono_info);

        // let singleton_function_id = FunId::new(self.env.symbol_pool().make("singleton"));
        let reverse_function_id = FunId::new(self.env.symbol_pool().make("reverse"));
        let append_function_id = FunId::new(self.env.symbol_pool().make("append"));
        let is_empty_function_id = FunId::new(self.env.symbol_pool().make("is_empty"));
        let contains_function_id = FunId::new(self.env.symbol_pool().make("contains"));
        let index_of_function_id = FunId::new(self.env.symbol_pool().make("index_of"));
        let remove_function_id = FunId::new(self.env.symbol_pool().make("remove"));
        let insert_function_id = FunId::new(self.env.symbol_pool().make("insert"));
        let swap_remove_function_id = FunId::new(self.env.symbol_pool().make("swap_remove"));
        let intrinsic_fun_ids: BTreeSet<_> = match self
            .env
            .find_module_by_name(self.env.symbol_pool().make("vector"))
        {
            Some(vector_module) => {
                vec![
                    // singleton_function_id,
                    reverse_function_id,
                    append_function_id,
                    is_empty_function_id,
                    contains_function_id,
                    index_of_function_id,
                    remove_function_id,
                    insert_function_id,
                    swap_remove_function_id,
                ]
                .into_iter()
                .map(|id| vector_module.get_id().qualified(id))
                .collect()
            }
            None => BTreeSet::new(),
        };

        let mut translated_types = BTreeSet::new();
        let mut verified_functions_count = 0;
        info!(
            "generating verification conditions for {:?} module(s)",
            self.env.get_module_count()
        );
        for module_env in self.env.get_modules() {
            self.writer.set_location(&module_env.env.internal_loc());

            self.spec_translator
                .translate_spec_vars(&module_env, mono_info.as_ref());
            self.spec_translator
                .translate_spec_funs(&module_env, mono_info.as_ref());

            for ref struct_env in module_env.get_structs() {
                if struct_env.is_native_or_intrinsic() {
                    continue;
                }
                for type_inst in mono_info
                    .structs
                    .get(&struct_env.get_qualified_id())
                    .unwrap_or(empty)
                {
                    let struct_name = boogie_struct_name(struct_env, type_inst);
                    if !translated_types.insert(struct_name) {
                        continue;
                    }
                    StructTranslator {
                        parent: self,
                        struct_env,
                        type_inst: type_inst.as_slice(),
                    }
                    .translate();
                }
            }

            for ref fun_env in module_env.get_functions() {
                if fun_env.is_native_or_intrinsic()
                    || intrinsic_fun_ids.contains(&fun_env.get_qualified_id())
                {
                    continue;
                }

                if self.targets.is_spec(&fun_env.get_qualified_id()) {
                    verified_functions_count += 1;

                    if self
                        .targets
                        .scenario_specs()
                        .contains(&fun_env.get_qualified_id())
                    {
                        if self.targets.has_target(
                            fun_env,
                            &FunctionVariant::Verification(VerificationFlavor::Regular),
                        ) {
                            let fun_target = self.targets.get_target(
                                fun_env,
                                &FunctionVariant::Verification(VerificationFlavor::Regular),
                            );
                            FunctionTranslator {
                                parent: self,
                                fun_target: &fun_target,
                                type_inst: &[],
                                style: FunctionTranslationStyle::Default,
                            }
                            .translate();
                        }
                        continue;
                    }

                    self.translate_function_style(fun_env, FunctionTranslationStyle::Default);
                    self.translate_function_style(fun_env, FunctionTranslationStyle::Asserts);
                    self.translate_function_style(fun_env, FunctionTranslationStyle::Aborts);
                    self.translate_function_style(
                        fun_env,
                        FunctionTranslationStyle::SpecNoAbortCheck,
                    );
                    self.translate_function_style(fun_env, FunctionTranslationStyle::Opaque);
                } else {
                    let fun_target = self.targets.get_target(fun_env, &FunctionVariant::Baseline);
                    if !verification_analysis::get_info(&fun_target).inlined {
                        continue;
                    }

                    if let Some(spec_qid) = self
                        .targets
                        .get_opaque_spec_by_fun(&fun_env.get_qualified_id())
                    {
                        if !self.targets.no_verify_specs().contains(spec_qid) {
                            FunctionTranslator {
                                parent: self,
                                fun_target: &fun_target,
                                type_inst: &[],
                                style: FunctionTranslationStyle::Default,
                            }
                            .translate();
                        }
                    } else {
                        // This variant is inlined, so translate for all type instantiations.
                        for type_inst in mono_info
                            .funs
                            .get(&(
                                fun_target.func_env.get_qualified_id(),
                                FunctionVariant::Baseline,
                            ))
                            .unwrap_or(&BTreeSet::new())
                        {
                            FunctionTranslator {
                                parent: self,
                                fun_target: &fun_target,
                                type_inst,
                                style: FunctionTranslationStyle::Default,
                            }
                            .translate();
                        }
                    }
                }
            }

            for ref struct_env in module_env.get_structs() {
                if struct_env.is_native_or_intrinsic() {
                    continue;
                }
                if let Some(inv_fun_id) = self
                    .targets
                    .get_inv_by_datatype(&struct_env.get_qualified_id())
                {
                    let inv_fun_env = self.env.get_function(*inv_fun_id);
                    let inv_fun_target = self
                        .targets
                        .get_target(&inv_fun_env, &FunctionVariant::Baseline);
                    let struct_type_instances = mono_info
                        .structs
                        .get(&struct_env.get_qualified_id())
                        .unwrap_or(empty);
                    let inv_fun_type_instances = mono_info
                        .funs
                        .get(&(inv_fun_env.get_qualified_id(), FunctionVariant::Baseline))
                        .unwrap_or(empty);
                    for type_inst in struct_type_instances.difference(inv_fun_type_instances) {
                        FunctionTranslator {
                            parent: self,
                            fun_target: &inv_fun_target,
                            type_inst,
                            style: FunctionTranslationStyle::Default,
                        }
                        .translate();
                    }
                }
            }
        }
        // Emit any finalization items required by spec translation.
        self.spec_translator.finalize();
        info!("{} verification conditions", verified_functions_count);
    }

    fn translate_function_style(&self, fun_env: &FunctionEnv, style: FunctionTranslationStyle) {
        use Bytecode::*;

        if style == FunctionTranslationStyle::Default
            && (self
                .get_verification_target_fun_env(&fun_env.get_qualified_id())
                .unwrap()
                .is_native()
                || self
                    .targets
                    .no_verify_specs()
                    .contains(&fun_env.get_qualified_id()))
        {
            return;
        }

        let requires_function =
            Operation::apply_fun_qid(&fun_env.module_env.env.requires_qid(), vec![]);
        let ensures_function =
            Operation::apply_fun_qid(&fun_env.module_env.env.ensures_qid(), vec![]);
        let asserts_function =
            Operation::apply_fun_qid(&fun_env.module_env.env.asserts_qid(), vec![]);
        let ensures_requires_swap_subst = BTreeMap::from_iter(vec![
            (requires_function.clone(), ensures_function.clone()),
            (ensures_function.clone(), requires_function.clone()),
        ]);
        let ensures_asserts_to_requires_subst = BTreeMap::from_iter(vec![
            (ensures_function.clone(), requires_function.clone()),
            (asserts_function.clone(), requires_function.clone()),
        ]);

        let variant = match style {
            FunctionTranslationStyle::Default | FunctionTranslationStyle::SpecNoAbortCheck => {
                FunctionVariant::Verification(VerificationFlavor::Regular)
            }
            FunctionTranslationStyle::Asserts
            | FunctionTranslationStyle::Aborts
            | FunctionTranslationStyle::Opaque => FunctionVariant::Baseline,
        };
        if variant.is_verified() && !self.targets.has_target(fun_env, &variant) {
            return;
        }
        let spec_fun_target = self.targets.get_target(&fun_env, &variant);
        if !variant.is_verified() && !verification_analysis::get_info(&spec_fun_target).inlined {
            return;
        }

        let mut builder =
            FunctionDataBuilder::new(spec_fun_target.func_env, spec_fun_target.data.clone());
        let code = std::mem::take(&mut builder.data.code);
        for bc in code.into_iter() {
            match style {
                FunctionTranslationStyle::Default => match bc {
                    Call(_, _, op, _, _) if op == asserts_function => {}
                    Call(_, _, Operation::Function(module_id, fun_id, _), _, _)
                        if self.targets.get_fun_by_opaque_spec(
                            &spec_fun_target.func_env.get_qualified_id(),
                        ) == Some(&QualifiedId {
                            module_id,
                            id: fun_id,
                        }) =>
                    {
                        builder.emit(bc)
                    }
                    _ => builder.emit(bc.update_abort_action(|_| None)),
                },
                FunctionTranslationStyle::Asserts | FunctionTranslationStyle::Aborts => match bc {
                    Call(_, _, op, _, _) if op == requires_function || op == ensures_function => {}
                    Call(_, _, Operation::Function(module_id, fun_id, _), _, _)
                        if self.targets.get_fun_by_opaque_spec(
                            &spec_fun_target.func_env.get_qualified_id(),
                        ) == Some(&QualifiedId {
                            module_id,
                            id: fun_id,
                        }) => {}
                    Ret(..) => {}
                    _ => builder.emit(bc.update_abort_action(|_| None)),
                },
                FunctionTranslationStyle::SpecNoAbortCheck => match bc {
                    Call(_, ref dests, Operation::Function(module_id, fun_id, _), ref srcs, _)
                        if self.targets.get_fun_by_opaque_spec(
                            &spec_fun_target.func_env.get_qualified_id(),
                        ) == Some(&QualifiedId {
                            module_id,
                            id: fun_id,
                        }) =>
                    {
                        let dests_clone = dests.clone();
                        let srcs_clone = srcs.clone();
                        builder.emit(bc.update_abort_action(|_| None));
                        let callee_fun_env = self.env.get_function(module_id.qualified(fun_id));
                        for (ret_idx, temp_idx) in dests_clone.iter().enumerate() {
                            let havoc_kind = if callee_fun_env
                                .get_return_type(ret_idx)
                                .is_mutable_reference()
                            {
                                HavocKind::MutationAll
                            } else {
                                HavocKind::Value
                            };
                            builder.emit_havoc(*temp_idx, havoc_kind);
                        }
                        for (param_idx, temp_idx) in srcs_clone.iter().enumerate() {
                            if callee_fun_env
                                .get_local_type(param_idx)
                                .is_mutable_reference()
                            {
                                builder.emit_havoc(*temp_idx, HavocKind::MutationValue);
                            };
                        }
                    }
                    Ret(..) => {}
                    _ => builder.emit(
                        bc.substitute_operations(&ensures_asserts_to_requires_subst)
                            .update_abort_action(|aa| match aa {
                                Some(AbortAction::Jump(_, _)) => Some(AbortAction::Check),
                                Some(AbortAction::Check) => Some(AbortAction::Check),
                                None => None,
                            }),
                    ),
                },
                FunctionTranslationStyle::Opaque => match bc {
                    Call(_, _, op, _, _) if op == asserts_function => {}
                    Call(_, ref dests, Operation::Function(module_id, fun_id, _), ref srcs, _)
                        if self.targets.get_fun_by_opaque_spec(
                            &spec_fun_target.func_env.get_qualified_id(),
                        ) == Some(&QualifiedId {
                            module_id,
                            id: fun_id,
                        }) =>
                    {
                        let dests_clone = dests.clone();
                        let srcs_clone = srcs.clone();
                        builder.emit(bc);
                        let callee_fun_env = self.env.get_function(module_id.qualified(fun_id));
                        for (ret_idx, temp_idx) in dests_clone.iter().enumerate() {
                            let havoc_kind = if callee_fun_env
                                .get_return_type(ret_idx)
                                .is_mutable_reference()
                            {
                                HavocKind::MutationValue
                            } else {
                                HavocKind::Value
                            };
                            builder.emit_havoc(*temp_idx, havoc_kind);
                        }
                        for (param_idx, temp_idx) in srcs_clone.iter().enumerate() {
                            if callee_fun_env
                                .get_local_type(param_idx)
                                .is_mutable_reference()
                            {
                                builder.emit_havoc(*temp_idx, HavocKind::MutationValue);
                            };
                        }
                    }
                    _ => builder.emit(
                        bc.substitute_operations(&ensures_requires_swap_subst)
                            .update_abort_action(|_| None),
                    ),
                },
            }
        }

        let mut data = builder.data;
        let reach_def = ReachingDefProcessor::new();
        let live_vars = LiveVarAnalysisProcessor::new_no_annotate();
        let mut dummy_targets = FunctionTargetsHolder::new();
        data = reach_def.process(&mut dummy_targets, builder.fun_env, data, None);
        data = live_vars.process(&mut dummy_targets, builder.fun_env, data, None);

        let fun_target = FunctionTarget::new(builder.fun_env, &data);
        if style == FunctionTranslationStyle::Default
            || style == FunctionTranslationStyle::Asserts
            || style == FunctionTranslationStyle::Aborts
            || style == FunctionTranslationStyle::SpecNoAbortCheck
            || style == FunctionTranslationStyle::Opaque
        // this is for the $opaque signature
        {
            FunctionTranslator {
                parent: self,
                fun_target: &fun_target,
                type_inst: &[],
                style,
            }
            .translate();
        }

        if style == FunctionTranslationStyle::Opaque || style == FunctionTranslationStyle::Aborts {
            mono_analysis::get_info(self.env)
                .funs
                .get(&(
                    *self
                        .targets
                        .get_fun_by_opaque_spec(&fun_target.func_env.get_qualified_id())
                        .unwrap(),
                    FunctionVariant::Baseline,
                ))
                .unwrap_or(&BTreeSet::new())
                .iter()
                .for_each(|type_inst| {
                    // Skip the none instantiation (i.e., each type parameter is
                    // instantiated to itself as a concrete type). This has the same
                    // effect as `type_inst: &[]` and is already captured above.
                    let is_none_inst = type_inst
                        .iter()
                        .enumerate()
                        .all(|(i, t)| matches!(t, Type::TypeParameter(idx) if *idx == i as u16));
                    if is_none_inst {
                        return;
                    }

                    FunctionTranslator {
                        parent: self,
                        fun_target: &fun_target,
                        type_inst,
                        style,
                    }
                    .translate();
                });
        }
    }

    fn translate_ghost_global(&mut self, mono_info: &std::rc::Rc<MonoInfo>) {
        let ghost_declare_global_type_instances = self
            .targets
            .specs()
            .map(|id| {
                spec_global_variable_analysis::get_info(
                    self.targets
                        .get_data(id, &FunctionVariant::Baseline)
                        .unwrap(),
                )
                .all_vars()
            })
            .flatten()
            .collect::<BTreeSet<_>>();
        let ghost_declare_global_mut_type_instances = self
            .targets
            .specs()
            .map(|id| {
                spec_global_variable_analysis::get_info(
                    self.targets
                        .get_data(id, &FunctionVariant::Baseline)
                        .unwrap(),
                )
                .mut_vars()
            })
            .flatten()
            .collect::<BTreeSet<_>>();

        if ghost_declare_global_type_instances.is_empty() {
            return;
        }

        let ghost_global_fun_env = self.env.get_function(self.env.global_qid());
        let ghost_global_fun_target = self
            .targets
            .get_target(&ghost_global_fun_env, &FunctionVariant::Baseline);
        let ghost_havoc_global_fun_env = self.env.get_function(self.env.havoc_global_qid());
        let ghost_havoc_global_fun_target = self
            .targets
            .get_target(&ghost_havoc_global_fun_env, &FunctionVariant::Baseline);

        let empty_set = &BTreeSet::new();
        let ghost_global_type_instances = mono_info
            .funs
            .get(&(
                ghost_global_fun_env.get_qualified_id(),
                FunctionVariant::Baseline,
            ))
            .unwrap_or(empty_set);

        assert!(
            ghost_global_type_instances.is_subset(&ghost_declare_global_type_instances),
            "missing type instances for function {}",
            ghost_global_fun_env.get_full_name_str(),
        );

        for type_inst in &ghost_declare_global_type_instances {
            self.generate_ghost_global_var_declaration(type_inst);
        }

        for type_inst in ghost_global_type_instances {
            FunctionTranslator {
                parent: self,
                fun_target: &ghost_global_fun_target,
                type_inst,
                style: FunctionTranslationStyle::Default,
            }
            .translate();
        }

        for type_inst in &ghost_declare_global_mut_type_instances {
            FunctionTranslator {
                parent: self,
                fun_target: &ghost_havoc_global_fun_target,
                type_inst,
                style: FunctionTranslationStyle::Default,
            }
            .translate();
        }
    }

    fn generate_ghost_global_var_declaration(&self, type_inst: &[Type]) {
        emitln!(
            self.writer,
            "{}",
            boogie_declare_global(
                self.env,
                &boogie_spec_global_var_name(self.env, type_inst),
                &type_inst[1],
            ),
        );
    }

    fn get_verification_target_fun_env(
        &self,
        spec_fun_qid: &QualifiedId<FunId>,
    ) -> Option<FunctionEnv> {
        self.targets
            .get_fun_by_opaque_spec(spec_fun_qid)
            .map(|qid| self.env.get_function(*qid))
    }
}

// =================================================================================================
// Struct Translation

impl<'env> StructTranslator<'env> {
    fn inst(&self, ty: &Type) -> Type {
        ty.instantiate(self.type_inst)
    }

    /// Return whether a field involves bitwise operations
    pub fn field_bv_flag(&self, field_id: &FieldId) -> bool {
        let global_state = &self
            .parent
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let operation_map = &global_state.struct_operation_map;
        let mid = self.struct_env.module_env.get_id();
        let sid = self.struct_env.get_id();
        let field_oper = operation_map.get(&(mid, sid)).unwrap().get(field_id);
        matches!(field_oper, Some(&Bitwise))
    }

    /// Return boogie type for a struct
    pub fn boogie_type_for_struct_field(
        &self,
        field_id: &FieldId,
        env: &GlobalEnv,
        ty: &Type,
    ) -> String {
        let bv_flag = self.field_bv_flag(field_id);
        if bv_flag {
            boogie_bv_type(env, ty)
        } else {
            boogie_type(env, ty)
        }
    }

    /// Translates the given struct.
    fn translate(&self) {
        let writer = self.parent.writer;
        let struct_env = self.struct_env;
        if struct_env.is_native() {
            return;
        }
        let env = struct_env.module_env.env;

        let qid = struct_env
            .get_qualified_id()
            .instantiate(self.type_inst.to_owned());
        emitln!(
            writer,
            "// struct {} {}",
            env.display(&qid),
            struct_env.get_loc().display(env)
        );

        // Set the location to internal as default.
        writer.set_location(&env.internal_loc());

        // Emit data type
        let struct_name = boogie_struct_name(struct_env, self.type_inst);
        emitln!(writer, "datatype {} {{", struct_name);

        // Emit constructor
        let fields = struct_env
            .get_fields()
            .map(|field| {
                format!(
                    "${}: {}",
                    field.get_name().display(env.symbol_pool()),
                    self.boogie_type_for_struct_field(
                        &field.get_id(),
                        env,
                        &self.inst(&field.get_type())
                    )
                )
            })
            .join(", ");
        emitln!(writer, "    {}({})", struct_name, fields,);
        emitln!(writer, "}");

        let suffix = boogie_type_suffix_for_struct(struct_env, self.type_inst, false);

        // Emit $UpdateField functions.
        let fields = struct_env.get_fields().collect_vec();
        for (pos, field_env) in fields.iter().enumerate() {
            let field_name = field_env.get_name().display(env.symbol_pool()).to_string();
            self.emit_function(
                &format!(
                    "$Update'{}'_{}(s: {}, x: {}): {}",
                    suffix,
                    field_name,
                    struct_name,
                    self.boogie_type_for_struct_field(
                        &field_env.get_id(),
                        env,
                        &self.inst(&field_env.get_type())
                    ),
                    struct_name
                ),
                || {
                    let args = fields
                        .iter()
                        .enumerate()
                        .map(|(p, f)| {
                            if p == pos {
                                "x".to_string()
                            } else {
                                format!("s->{}", boogie_field_sel(f, self.type_inst))
                            }
                        })
                        .join(", ");
                    emitln!(writer, "{}({})", struct_name, args);
                },
            );
        }

        // Emit $IsValid function.
        self.emit_function_with_attr(
            "", // not inlined!
            &format!("$IsValid'{}'(s: {}): bool", suffix, struct_name),
            || {
                if struct_env.is_native_or_intrinsic() {
                    emitln!(writer, "true")
                } else {
                    let mut sep = "";
                    for field in struct_env.get_fields() {
                        let sel = format!("s->{}", boogie_field_sel(&field, self.type_inst));
                        let ty = &field.get_type().instantiate(self.type_inst);
                        let bv_flag = self.field_bv_flag(&field.get_id());
                        emitln!(
                            writer,
                            "{}{}",
                            sep,
                            boogie_well_formed_expr_bv(env, &sel, ty, bv_flag)
                        );
                        sep = "  && ";
                    }
                }
            },
        );

        // Emit equality
        self.emit_function(
            &format!(
                "$IsEqual'{}'(s1: {}, s2: {}): bool",
                suffix, struct_name, struct_name
            ),
            || {
                if struct_has_native_equality(struct_env, self.type_inst, self.parent.options) {
                    emitln!(writer, "s1 == s2")
                } else {
                    let mut sep = "";
                    for field in &fields {
                        let sel_fun = boogie_field_sel(field, self.type_inst);
                        let bv_flag = self.field_bv_flag(&field.get_id());
                        let field_suffix =
                            boogie_type_suffix_bv(env, &self.inst(&field.get_type()), bv_flag);
                        emit!(
                            writer,
                            "{}$IsEqual'{}'(s1->{}, s2->{})",
                            sep,
                            field_suffix,
                            sel_fun,
                            sel_fun,
                        );
                        sep = "\n&& ";
                    }
                }
            },
        );

        if struct_env.has_memory() {
            // Emit memory variable.
            let memory_name = boogie_resource_memory_name(
                env,
                &struct_env
                    .get_qualified_id()
                    .instantiate(self.type_inst.to_owned()),
                &None,
            );
            emitln!(writer, "var {}: $Memory {};", memory_name, struct_name);
        }

        emitln!(
            writer,
            "procedure {{:inline 1}} $0_prover_type_inv'{}'(s: {}) returns (res: bool) {{",
            suffix,
            struct_name
        );
        writer.indent();
        if let Some(inv_fun_id) = self
            .parent
            .targets
            .get_inv_by_datatype(&self.struct_env.get_qualified_id())
        {
            emitln!(
                writer,
                "call res := {}(s);",
                boogie_function_name(
                    &self.parent.env.get_function(*inv_fun_id),
                    self.type_inst,
                    FunctionTranslationStyle::Default
                )
            );
        } else {
            emitln!(writer, "res := true;");
        }
        emitln!(writer, "return;");
        writer.unindent();
        emitln!(writer, "}");

        emitln!(writer);
    }

    fn emit_function(&self, signature: &str, body_fn: impl Fn()) {
        self.emit_function_with_attr("{:inline} ", signature, body_fn)
    }

    fn emit_function_with_attr(&self, attr: &str, signature: &str, body_fn: impl Fn()) {
        let writer = self.parent.writer;
        emitln!(writer, "function {}{} {{", attr, signature);
        writer.indent();
        body_fn();
        writer.unindent();
        emitln!(writer, "}");
    }
}

// =================================================================================================
// Function Translation

impl<'env> FunctionTranslator<'env> {
    /// Return whether a specific TempIndex involves in bitwise operations
    pub fn bv_flag_from_map(&self, i: &usize, operation_map: &FuncOperationMap) -> bool {
        let mid = self.fun_target.module_env().get_id();
        let sid = self.fun_target.func_env.get_id();
        let param_oper = operation_map.get(&(mid, sid)).unwrap().get(i);
        matches!(param_oper, Some(&Bitwise))
    }

    /// Return whether a specific TempIndex involves in bitwise operations
    pub fn bv_flag(&self, num_oper: &NumOperation) -> bool {
        *num_oper == Bitwise
    }

    /// Return whether a return value at position i involves in bitwise operation
    pub fn ret_bv_flag(&self, i: &usize) -> bool {
        let global_state = &self
            .fun_target
            .global_env()
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let operation_map = &global_state.get_ret_map();
        self.bv_flag_from_map(i, operation_map)
    }

    /// Return boogie type for a local with given signature token.
    pub fn boogie_type_for_fun(
        &self,
        env: &GlobalEnv,
        ty: &Type,
        num_oper: &NumOperation,
    ) -> String {
        let bv_flag = self.bv_flag(num_oper);
        if bv_flag {
            boogie_bv_type(env, ty)
        } else {
            boogie_type(env, ty)
        }
    }

    fn inst(&self, ty: &Type) -> Type {
        ty.instantiate(self.type_inst)
    }

    fn inst_slice(&self, tys: &[Type]) -> Vec<Type> {
        tys.iter().map(|ty| self.inst(ty)).collect()
    }

    fn get_local_type(&self, idx: TempIndex) -> Type {
        self.fun_target
            .get_local_type(idx)
            .instantiate(self.type_inst)
    }

    /// Translates the given function.
    fn translate(mut self) {
        let writer = self.parent.writer;
        let fun_target = self.fun_target;
        let env = fun_target.global_env();
        let qid = fun_target
            .func_env
            .get_qualified_id()
            .instantiate(self.type_inst.to_owned());
        emitln!(
            writer,
            "// fun {} [{}] {}",
            env.display(&qid),
            fun_target.data.variant,
            fun_target.get_loc().display(env)
        );
        self.generate_function_sig();

        if self.fun_target.func_env.get_qualified_id() == self.parent.env.global_qid() {
            self.generate_ghost_global_body();
        } else if self.fun_target.func_env.get_qualified_id() == self.parent.env.havoc_global_qid()
        {
            self.generate_ghost_havoc_global_body();
        } else {
            self.generate_function_body();
        }
        emitln!(self.parent.writer);
    }

    fn generate_ghost_global_body(&self) {
        assert!(
            self.fun_target.func_env.is_native()
                && self.fun_target.get_type_parameter_count() == 2
                && self.fun_target.get_parameter_count() == 0
                && self.fun_target.get_return_count() == 1
        );
        emitln!(self.writer(), "{");
        self.writer().indent();
        emitln!(
            self.writer(),
            "$ret0 := {};",
            boogie_spec_global_var_name(self.parent.env, self.type_inst),
        );
        self.writer().unindent();
        emitln!(self.writer(), "}");
    }

    fn generate_ghost_havoc_global_body(&self) {
        assert!(
            self.fun_target.func_env.is_native()
                && self.fun_target.get_type_parameter_count() == 2
                && self.fun_target.get_parameter_count() == 0
                && self.fun_target.get_return_count() == 0
        );
        emitln!(self.writer(), "{");
        self.writer().indent();
        emitln!(
            self.writer(),
            "havoc {};",
            boogie_spec_global_var_name(self.parent.env, self.type_inst),
        );
        self.writer().unindent();
        emitln!(self.writer(), "}");
    }

    fn function_variant_name(&self, style: FunctionTranslationStyle) -> String {
        let variant = match style {
            FunctionTranslationStyle::Default => &self.fun_target.data.variant,
            FunctionTranslationStyle::Asserts
            | FunctionTranslationStyle::Aborts
            | FunctionTranslationStyle::Opaque => &FunctionVariant::Baseline,
            FunctionTranslationStyle::SpecNoAbortCheck => {
                &FunctionVariant::Verification(VerificationFlavor::Regular)
            }
        };
        let suffix = match variant {
            FunctionVariant::Baseline => "".to_string(),
            FunctionVariant::Verification(flavor) => match flavor {
                VerificationFlavor::Regular => "$verify".to_string(),
                VerificationFlavor::Instantiated(_) => {
                    format!("$verify_{}", flavor)
                }
                VerificationFlavor::Inconsistency(_) => {
                    format!("$verify_{}", flavor)
                }
            },
        };
        if self
            .parent
            .targets
            .get_opaque_spec_by_fun(&self.fun_target.func_env.get_qualified_id())
            .is_some()
            && style == FunctionTranslationStyle::Default
        {
            return format!(
                "{}$impl",
                boogie_function_name(self.fun_target.func_env, self.type_inst, style)
            );
        }
        let fun_name = self
            .parent
            .targets
            .get_fun_by_opaque_spec(&self.fun_target.func_env.get_qualified_id())
            .map_or(
                boogie_function_name(self.fun_target.func_env, self.type_inst, style),
                |fun_id| {
                    boogie_function_name(
                        &self.parent.env.get_function(*fun_id),
                        self.type_inst,
                        style,
                    )
                },
            );
        format!("{}{}", fun_name, suffix)
    }

    /// Return a string for a boogie procedure header. Use inline attribute and name
    /// suffix as indicated by `entry_point`.
    fn generate_function_sig(&self) {
        let writer = self.parent.writer;
        let options = self.parent.options;
        let fun_target = self.fun_target;
        let (args, prerets) = self.generate_function_args_and_returns();

        let attribs = match &fun_target.data.variant {
            FunctionVariant::Baseline => "{:inline 1} ".to_string(),
            FunctionVariant::Verification(flavor) => {
                let timeout = fun_target
                    .func_env
                    .get_num_pragma(TIMEOUT_PRAGMA, || options.vc_timeout);
                let mut attribs = vec![format!("{{:timeLimit {}}} ", timeout)];
                if fun_target.func_env.is_num_pragma_set(SEED_PRAGMA) {
                    let seed = fun_target
                        .func_env
                        .get_num_pragma(SEED_PRAGMA, || options.random_seed);
                    attribs.push(format!("{{:random_seed {}}} ", seed));
                };
                match flavor {
                    VerificationFlavor::Regular => "".to_string(),
                    VerificationFlavor::Instantiated(_) => "".to_string(),
                    VerificationFlavor::Inconsistency(_) => {
                        attribs.push(format!(
                            "{{:msg_if_verifies \"inconsistency_detected{}\"}} ",
                            self.loc_str(&fun_target.get_loc())
                        ));
                        "".to_string()
                    }
                };
                attribs.join("")
            }
        };

        let rets = match self.style {
            FunctionTranslationStyle::Default | FunctionTranslationStyle::Opaque => prerets,
            FunctionTranslationStyle::Asserts => "".to_string(),
            FunctionTranslationStyle::Aborts => "res: bool".to_string(),
            FunctionTranslationStyle::SpecNoAbortCheck => "".to_string(),
        };

        writer.set_location(&fun_target.get_loc());
        if self.style == FunctionTranslationStyle::Opaque {
            emitln!(
                writer,
                "procedure {}$opaque({}) returns ({});",
                self.function_variant_name(FunctionTranslationStyle::Opaque),
                args,
                rets,
            );
            emitln!(writer, "");
        }
        emitln!(
            writer,
            "procedure {}{}({}) returns ({})",
            attribs,
            self.function_variant_name(self.style),
            args,
            rets,
        )
    }

    /// Generate boogie representation of function args and return args.
    fn generate_function_args_and_returns(&self) -> (String, String) {
        let fun_target = self.fun_target;
        let env = fun_target.global_env();
        let baseline_flag = self.fun_target.data.variant == FunctionVariant::Baseline;
        let global_state = &self
            .fun_target
            .global_env()
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let mid = fun_target.func_env.module_env.get_id();
        let fid = fun_target.func_env.get_id();
        let args = (0..fun_target.get_parameter_count())
            .map(|i| {
                let ty = self.get_local_type(i);
                // Boogie does not allow to assign to parameters, so we need to proxy them.
                let prefix = if self.parameter_needs_to_be_mutable(fun_target, i) {
                    "_$"
                } else {
                    "$"
                };
                let num_oper = global_state
                    .get_temp_index_oper(mid, fid, i, baseline_flag)
                    .unwrap_or(&Bottom);
                format!(
                    "{}t{}: {}",
                    prefix,
                    i,
                    self.boogie_type_for_fun(env, &ty, num_oper)
                )
            })
            .join(", ");
        let mut_ref_inputs = (0..fun_target.get_parameter_count())
            .enumerate()
            .filter_map(|(i, idx)| {
                let ty = self.get_local_type(idx);
                if ty.is_mutable_reference() {
                    Some((i, ty))
                } else {
                    None
                }
            })
            .collect_vec();
        let rets = fun_target
            .get_return_types()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let s = self.inst(s);
                let operation_map = global_state.get_ret_map();
                let num_oper = operation_map.get(&(mid, fid)).unwrap().get(&i).unwrap();
                format!("$ret{}: {}", i, self.boogie_type_for_fun(env, &s, num_oper))
            })
            // Add implicit return parameters for &mut
            .chain(mut_ref_inputs.into_iter().enumerate().map(|(i, (_, ty))| {
                let num_oper = &global_state
                    .get_temp_index_oper(mid, fid, i, baseline_flag)
                    .unwrap();
                format!(
                    "$ret{}: {}",
                    usize::saturating_add(fun_target.get_return_count(), i),
                    self.boogie_type_for_fun(env, &ty, num_oper)
                )
            }))
            .join(", ");
        (args, rets)
    }

    /// Generates boogie implementation body.
    fn generate_function_body(&mut self) {
        let writer = self.parent.writer;
        let fun_target = self.fun_target;
        let variant = &fun_target.data.variant;
        let instantiation = &fun_target.data.type_args;
        let env = fun_target.global_env();
        let baseline_flag = self.fun_target.data.variant == FunctionVariant::Baseline;
        let global_state = &self
            .fun_target
            .global_env()
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");

        // Be sure to set back location to the whole function definition as a default.
        writer.set_location(&fun_target.get_loc().at_start());

        emitln!(writer, "{");
        writer.indent();

        // Print instantiation information
        if !instantiation.is_empty() {
            let display_ctxt = TypeDisplayContext::WithEnv {
                env,
                type_param_names: None,
            };
            emitln!(
                writer,
                "// function instantiation <{}>",
                instantiation
                    .iter()
                    .map(|ty| ty.display(&display_ctxt))
                    .join(", ")
            );
            emitln!(writer, "");
        }

        // Generate local variable declarations. They need to appear first in boogie.
        emitln!(writer, "// declare local variables");
        let num_args = fun_target.get_parameter_count();
        let mid = fun_target.func_env.module_env.get_id();
        let fid = fun_target.func_env.get_id();
        for i in num_args..fun_target.get_local_count() {
            let num_oper = global_state
                .get_temp_index_oper(mid, fid, i, baseline_flag)
                .unwrap_or_else(|| {
                    panic!(
                        "missing number operation info for function={}, temp {}",
                        self.fun_target.func_env.get_full_name_str(),
                        i
                    )
                });
            let local_type = &self.get_local_type(i);
            emitln!(
                writer,
                "var $t{}: {};",
                i,
                self.boogie_type_for_fun(env, local_type, num_oper)
            );
        }
        // Generate declarations for renamed parameters.
        let proxied_parameters = self.get_mutable_parameters();
        for (idx, ty) in &proxied_parameters {
            let num_oper = &global_state
                .get_temp_index_oper(mid, fid, *idx, baseline_flag)
                .unwrap();
            emitln!(
                writer,
                "var $t{}: {};",
                idx,
                self.boogie_type_for_fun(env, &ty.instantiate(self.type_inst), num_oper)
            );
        }
        // Generate declarations for modifies condition.
        let mut mem_inst_seen = BTreeSet::new();
        for qid in fun_target.get_modify_ids() {
            let memory = qid.instantiate(self.type_inst);
            if !mem_inst_seen.contains(&memory) {
                emitln!(
                    writer,
                    "var {}: {}",
                    boogie_modifies_memory_name(fun_target.global_env(), &memory),
                    "[int]bool;"
                );
                mem_inst_seen.insert(memory);
            }
        }
        let mut dup: Vec<String> = vec![];
        // Declare temporaries for debug tracing and other purposes.
        for (_, (ty, ref bv_flag, cnt)) in self.compute_needed_temps() {
            for i in 0..cnt {
                let bv_type = if *bv_flag {
                    boogie_bv_type
                } else {
                    boogie_type
                };
                let temp_name =
                    boogie_temp_from_suffix(env, &boogie_type_suffix_bv(env, &ty, *bv_flag), i);
                if !dup.contains(&temp_name) {
                    emitln!(writer, "var {}: {};", temp_name.clone(), bv_type(env, &ty));
                    dup.push(temp_name);
                }
            }
        }
        emitln!(writer, "var $abort_if_cond: bool;");

        // Generate memory snapshot variable declarations.
        let code = fun_target.get_bytecode();
        let labels = code
            .iter()
            .filter_map(|bc| {
                use Bytecode::*;
                match bc {
                    SaveMem(_, lab, mem) => Some((lab, mem)),
                    SaveSpecVar(..) => panic!("spec var memory snapshots NYI"),
                    _ => None,
                }
            })
            .collect::<BTreeSet<_>>();
        for (lab, mem) in labels {
            let mem = &mem.to_owned().instantiate(self.type_inst);
            let name = boogie_resource_memory_name(env, mem, &Some(*lab));
            emitln!(
                writer,
                "var {}: $Memory {};",
                name,
                boogie_struct_name(&env.get_struct_qid(mem.to_qualified_id()), &mem.inst)
            );
        }

        // Initialize renamed parameters.
        for (idx, _) in proxied_parameters {
            emitln!(writer, "$t{} := _$t{};", idx, idx);
        }

        // Initial assumptions
        if variant.is_verified() {
            self.translate_verify_entry_assumptions(fun_target);
        }

        // Initial value of res when generating abort condition
        if FunctionTranslationStyle::Aborts == self.style {
            emitln!(writer, "res := true;");
        }

        // Generate bytecode
        emitln!(writer, "\n// bytecode translation starts here");
        let mut last_tracked_loc = None;
        for bytecode in code.iter() {
            self.translate_bytecode(&mut last_tracked_loc, bytecode);
        }

        writer.unindent();
        emitln!(writer, "}");
    }

    fn get_mutable_parameters(&self) -> Vec<(TempIndex, Type)> {
        let fun_target = self.fun_target;
        (0..fun_target.get_parameter_count())
            .filter_map(|i| {
                if self.parameter_needs_to_be_mutable(fun_target, i) {
                    Some((i, fun_target.get_local_type(i).clone()))
                } else {
                    None
                }
            })
            .collect_vec()
    }

    /// Determines whether the parameter of a function needs to be mutable.
    /// Boogie does not allow to assign to procedure parameters. In some cases
    /// (e.g. for memory instrumentation, but also as a result of copy propagation),
    /// we may need to assign to parameters.
    fn parameter_needs_to_be_mutable(
        &self,
        _fun_target: &FunctionTarget<'_>,
        _idx: TempIndex,
    ) -> bool {
        // For now, we just always say true. This could be optimized because the actual (known
        // so far) sources for mutability are parameters which are used in WriteBack(LocalRoot(p))
        // position.
        true
    }

    fn translate_verify_entry_assumptions(&self, fun_target: &FunctionTarget<'_>) {
        let writer = self.parent.writer;
        emitln!(writer, "\n// verification entrypoint assumptions");

        // Prelude initialization
        emitln!(writer, "call $InitVerification();");

        // Assume reference parameters to be based on the Param(i) Location, ensuring
        // they are disjoint from all other references. This prevents aliasing and is justified as
        // follows:
        // - for mutual references, by their exclusive access in Move.
        // - for immutable references because we have eliminated them
        for i in 0..fun_target.get_parameter_count() {
            let ty = fun_target.get_local_type(i);
            if ty.is_reference() {
                emitln!(writer, "assume $t{}->l == $Param({});", i, i);
            }
        }
    }
}

// =================================================================================================
// Bytecode Translation

impl<'env> FunctionTranslator<'env> {
    fn writer(&self) -> &CodeWriter {
        self.parent.writer
    }

    /// Translates one bytecode instruction.
    fn translate_bytecode(
        &mut self,
        last_tracked_loc: &mut Option<(Loc, LineIndex)>,
        bytecode: &Bytecode,
    ) {
        use Bytecode::*;

        let spec_translator = &self.parent.spec_translator;
        let options = self.parent.options;
        let fun_target = self.fun_target;
        let env = fun_target.global_env();

        // Set location of this code in the CodeWriter.
        let attr_id = bytecode.get_attr_id();
        let loc = fun_target.get_bytecode_loc(attr_id);
        self.writer().set_location(&loc);

        // Print location.
        emitln!(
            self.writer(),
            "// {} {}",
            bytecode.display(fun_target, &BTreeMap::default()),
            loc.display(env)
        );

        // Print debug comments.
        if let Some(comment) = fun_target.get_debug_comment(attr_id) {
            if comment.starts_with("info: ") {
                // if the comment is annotated with "info: ", it should be displayed to the user
                emitln!(
                    self.writer(),
                    "assume {{:print \"${}(){}\"}} true;",
                    &comment[..4],
                    &comment[4..]
                );
            } else {
                emitln!(self.writer(), "// {}", comment);
            }
        }

        // Track location for execution traces.
        if matches!(bytecode, Call(_, _, Operation::TraceAbort, ..)) {
            // Ensure that aborts always has the precise location instead of the
            // line-approximated one
            *last_tracked_loc = None;
        }
        self.track_loc(last_tracked_loc, &loc);
        if matches!(bytecode, Label(_, _)) {
            // For labels, retrack the location after the label itself, so
            // the information will not be missing if we jump to this label
            *last_tracked_loc = None;
        }

        // Helper function to get a a string for a local
        let str_local = |idx: usize| format!("$t{}", idx);
        let baseline_flag = self.fun_target.data.variant == FunctionVariant::Baseline;
        let global_state = &self
            .fun_target
            .global_env()
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let mid = self.fun_target.func_env.module_env.get_id();
        let fid = self.fun_target.func_env.get_id();

        // Translate the bytecode instruction.
        match bytecode {
            SaveMem(_, label, mem) => {
                let mem = &mem.to_owned().instantiate(self.type_inst);
                let snapshot = boogie_resource_memory_name(env, mem, &Some(*label));
                let current = boogie_resource_memory_name(env, mem, &None);
                emitln!(self.writer(), "{} := {};", snapshot, current);
            }
            SaveSpecVar(_, _label, _var) => {
                panic!("spec var snapshot NYI")
            }
            Prop(id, kind, exp) => match kind {
                PropKind::Assert => {
                    emit!(self.writer(), "assert ");
                    let info = fun_target
                        .get_vc_info(*id)
                        .map(|s| s.as_str())
                        .unwrap_or("unknown assertion failed");
                    emit!(
                        self.writer(),
                        "{{:msg \"assert_failed{}: {}\"}}\n  ",
                        self.loc_str(&loc),
                        info
                    );
                    spec_translator.translate(exp, self.type_inst);
                    emitln!(self.writer(), ";");
                }
                PropKind::Assume => {
                    emit!(self.writer(), "assume ");
                    spec_translator.translate(exp, self.type_inst);
                    emitln!(self.writer(), ";");
                }
                PropKind::Modifies => {
                    let ty = &self.inst(&env.get_node_type(exp.node_id()));
                    let bv_flag = global_state.get_node_num_oper(exp.node_id()) == Bitwise;
                    let (mid, sid, inst) = ty.require_struct();
                    let memory = boogie_resource_memory_name(
                        env,
                        &mid.qualified_inst(sid, inst.to_owned()),
                        &None,
                    );
                    let exists_str = boogie_temp(env, &BOOL_TYPE, 0, false);
                    emitln!(self.writer(), "havoc {};", exists_str);
                    emitln!(self.writer(), "if ({}) {{", exists_str);
                    self.writer().with_indent(|| {
                        let val_str = boogie_temp(env, ty, 0, bv_flag);
                        emitln!(self.writer(), "havoc {};", val_str);
                        emit!(self.writer(), "{} := $ResourceUpdate({}, ", memory, memory);
                        spec_translator.translate(&exp.call_args()[0], self.type_inst);
                        emitln!(self.writer(), ", {});", val_str);
                    });
                    emitln!(self.writer(), "} else {");
                    self.writer().with_indent(|| {
                        emit!(self.writer(), "{} := $ResourceRemove({}, ", memory, memory);
                        spec_translator.translate(&exp.call_args()[0], self.type_inst);
                        emitln!(self.writer(), ");");
                    });
                    emitln!(self.writer(), "}");
                }
            },
            Label(_, label) => {
                self.writer().unindent();
                emitln!(self.writer(), "L{}:", label.as_usize());
                self.writer().indent();
            }
            Jump(_, target) => emitln!(self.writer(), "goto L{};", target.as_usize()),
            Branch(_, then_target, else_target, idx) => emitln!(
                self.writer(),
                "if ({}) {{ goto L{}; }} else {{ goto L{}; }}",
                str_local(*idx),
                then_target.as_usize(),
                else_target.as_usize(),
            ),
            VariantSwitch(..) => unimplemented!("translating variant_switch to Boogie"),
            Assign(_, dest, src, _) => {
                emitln!(
                    self.writer(),
                    "{} := {};",
                    str_local(*dest),
                    str_local(*src)
                );
            }
            Ret(_, rets) => {
                if FunctionTranslationStyle::Default == self.style
                    && self.fun_target.data.variant
                        == FunctionVariant::Verification(VerificationFlavor::Regular)
                    && self
                        .parent
                        .targets
                        .get_fun_by_opaque_spec(&self.fun_target.func_env.get_qualified_id())
                        .is_some()
                    && !self
                        .parent
                        .targets
                        .no_asserts()
                        .contains(&self.fun_target.func_env.get_qualified_id())
                {
                    emitln!(
                        self.writer(),
                        "call {}({});",
                        self.function_variant_name(FunctionTranslationStyle::Asserts),
                        (0..fun_target.get_parameter_count())
                            .map(|i| {
                                let prefix = if self.parameter_needs_to_be_mutable(fun_target, i) {
                                    "_$"
                                } else {
                                    "$"
                                };
                                format!("{}t{}", prefix, i)
                            })
                            .join(", "),
                    );
                }
                for (i, r) in rets.iter().enumerate() {
                    emitln!(self.writer(), "$ret{} := {};", i, str_local(*r));
                }
                // Also assign input to output $mut parameters
                let mut ret_idx = rets.len();
                for i in 0..fun_target.get_parameter_count() {
                    if self.get_local_type(i).is_mutable_reference() {
                        emitln!(self.writer(), "$ret{} := {};", ret_idx, str_local(i));
                        ret_idx = usize::saturating_add(ret_idx, 1);
                    }
                }
                emitln!(self.writer(), "return;");
            }
            Load(_, dest, c) => {
                let num_oper = global_state
                    .get_temp_index_oper(mid, fid, *dest, baseline_flag)
                    .unwrap();
                let bv_flag = self.bv_flag(num_oper);
                let value = match c {
                    Constant::Bool(true) => "true".to_string(),
                    Constant::Bool(false) => "false".to_string(),
                    Constant::U8(num) => boogie_num_literal(&num.to_string(), 8, bv_flag),
                    Constant::U64(num) => boogie_num_literal(&num.to_string(), 64, bv_flag),
                    Constant::U128(num) => boogie_num_literal(&num.to_string(), 128, bv_flag),
                    Constant::U256(num) => boogie_num_literal(&num.to_string(), 256, bv_flag),
                    Constant::Address(val) => val.to_string(),
                    Constant::ByteArray(val) => boogie_byte_blob(options, val, bv_flag),
                    Constant::AddressArray(val) => boogie_address_blob(options, val),
                    Constant::Vector(val) => boogie_constant_blob(options, val),
                    Constant::U16(num) => boogie_num_literal(&num.to_string(), 16, bv_flag),
                    Constant::U32(num) => boogie_num_literal(&num.to_string(), 32, bv_flag),
                };
                let dest_str = str_local(*dest);
                emitln!(self.writer(), "{} := {};", dest_str, value);
                // Insert a WellFormed assumption so the new value gets tagged as u8, ...
                let ty = &self.get_local_type(*dest);
                let check = boogie_well_formed_check(env, &dest_str, ty, bv_flag);
                if !check.is_empty() {
                    emitln!(self.writer(), &check);
                }
            }
            Call(_, dests, oper, srcs, aa) => {
                use Operation::*;
                match oper {
                    FreezeRef => unreachable!(),
                    UnpackRef | UnpackRefDeep | PackRef | PackRefDeep => {
                        // No effect
                    }
                    OpaqueCallBegin(_, _, _) | OpaqueCallEnd(_, _, _) => {
                        // These are just markers.  There is no generated code.
                    }
                    WriteBack(node, edge) => {
                        self.translate_write_back(node, edge, srcs[0]);
                    }
                    IsParent(node, edge) => {
                        if let BorrowNode::Reference(parent) = node {
                            let src_str = str_local(srcs[0]);
                            let edge_pattern = edge
                                .flatten()
                                .into_iter()
                                .filter_map(|e| match e {
                                    BorrowEdge::Field(_, offset) => Some(format!("{}", offset)),
                                    BorrowEdge::Index(_) => Some("-1".to_owned()),
                                    BorrowEdge::Direct => None,
                                    _ => unreachable!(),
                                })
                                .collect_vec();
                            if edge_pattern.is_empty() {
                                emitln!(
                                    self.writer(),
                                    "{} := $IsSameMutation({}, {});",
                                    str_local(dests[0]),
                                    str_local(*parent),
                                    src_str
                                );
                            } else if edge_pattern.len() == 1 {
                                emitln!(
                                    self.writer(),
                                    "{} := $IsParentMutation({}, {}, {});",
                                    str_local(dests[0]),
                                    str_local(*parent),
                                    edge_pattern[0],
                                    src_str
                                );
                            } else {
                                emitln!(
                                    self.writer(),
                                    "{} := $IsParentMutationHyper({}, {}, {});",
                                    str_local(dests[0]),
                                    str_local(*parent),
                                    boogie_make_vec_from_strings(&edge_pattern),
                                    src_str
                                );
                            }
                        } else {
                            panic!("inconsistent IsParent instruction: expected a reference node")
                        }
                    }
                    PackVariant(..) => unimplemented!("translating pack_variant to Boogie"),
                    UnpackVariant(..) => unimplemented!("translating pack_variant to Boogie"),
                    BorrowLoc => {
                        let src = srcs[0];
                        let dest = dests[0];
                        emitln!(
                            self.writer(),
                            "{} := $Mutation($Local({}), EmptyVec(), {});",
                            str_local(dest),
                            src,
                            str_local(src)
                        );
                    }
                    ReadRef => {
                        let src = srcs[0];
                        let dest = dests[0];
                        emitln!(
                            self.writer(),
                            "{} := $Dereference({});",
                            str_local(dest),
                            str_local(src)
                        );
                    }
                    WriteRef => {
                        let reference = srcs[0];
                        let value = srcs[1];
                        emitln!(
                            self.writer(),
                            "{} := $UpdateMutation({}, {});",
                            str_local(reference),
                            str_local(reference),
                            str_local(value),
                        );
                    }
                    Function(mid, fid, inst) => {
                        let inst = &self.inst_slice(inst);
                        let module_env = env.get_module(*mid);
                        let callee_env = module_env.get_function(*fid);

                        let mut args_str = srcs.iter().cloned().map(str_local).join(", ");
                        let dest_str = dests
                            .iter()
                            .cloned()
                            .map(str_local)
                            // Add implict dest returns for &mut srcs:
                            //  f(x) --> x := f(x)  if type(x) = &mut_
                            .chain(
                                srcs.iter()
                                    .filter(|idx| self.get_local_type(**idx).is_mutable_reference())
                                    .cloned()
                                    .map(str_local),
                            )
                            .join(",");

                        // special casing for type reflection
                        let mut processed = false;

                        // TODO(mengxu): change it to a better address name instead of extlib
                        if env.get_extlib_address() == *module_env.get_name().addr() {
                            let qualified_name = format!(
                                "{}::{}",
                                module_env.get_name().name().display(env.symbol_pool()),
                                callee_env.get_name().display(env.symbol_pool()),
                            );
                            if qualified_name == TYPE_NAME_MOVE {
                                assert_eq!(inst.len(), 1);
                                if dest_str.is_empty() {
                                    emitln!(
                                        self.writer(),
                                        "{}",
                                        boogie_reflection_type_name(env, &inst[0], false)
                                    );
                                } else {
                                    emitln!(
                                        self.writer(),
                                        "{} := {};",
                                        dest_str,
                                        boogie_reflection_type_name(env, &inst[0], false)
                                    );
                                }
                                processed = true;
                            } else if qualified_name == TYPE_INFO_MOVE {
                                assert_eq!(inst.len(), 1);
                                let (flag, info) = boogie_reflection_type_info(env, &inst[0]);
                                emitln!(self.writer(), "if (!{}) {{", flag);
                                self.writer().with_indent(|| {
                                    emitln!(self.writer(), "call $ExecFailureAbort();")
                                });
                                emitln!(self.writer(), "}");
                                if !dest_str.is_empty() {
                                    emitln!(self.writer(), "else {");
                                    self.writer().with_indent(|| {
                                        emitln!(self.writer(), "{} := {};", dest_str, info)
                                    });
                                    emitln!(self.writer(), "}");
                                }
                                processed = true;
                            }
                        }

                        if env.get_stdlib_address() == *module_env.get_name().addr() {
                            let qualified_name = format!(
                                "{}::{}",
                                module_env.get_name().name().display(env.symbol_pool()),
                                callee_env.get_name().display(env.symbol_pool()),
                            );
                            if qualified_name == TYPE_NAME_GET_MOVE {
                                assert_eq!(inst.len(), 1);
                                if dest_str.is_empty() {
                                    emitln!(
                                        self.writer(),
                                        "{}",
                                        boogie_reflection_type_name(env, &inst[0], true)
                                    );
                                } else {
                                    emitln!(
                                        self.writer(),
                                        "{} := {};",
                                        dest_str,
                                        boogie_reflection_type_name(env, &inst[0], true)
                                    );
                                }
                                processed = true;
                            }
                        }

                        if callee_env.get_qualified_id() == self.parent.env.asserts_qid()
                            && self.style == FunctionTranslationStyle::Aborts
                        {
                            emitln!(self.writer(), "res := {};", args_str);
                            emitln!(self.writer(), "if (!res) { return; }");
                            processed = true;
                        }

                        if self
                            .parent
                            .targets
                            .get_fun_by_opaque_spec(&self.fun_target.func_env.get_qualified_id())
                            == Some(&mid.qualified(*fid))
                            && self.style == FunctionTranslationStyle::Opaque
                        {
                            if self
                                .parent
                                .targets
                                .no_asserts()
                                .contains(&self.fun_target.func_env.get_qualified_id())
                            {
                                emitln!(self.writer(), "havoc $abort_flag;");
                            } else {
                                emitln!(
                                    self.writer(),
                                    "call $abort_if_cond := {}({});",
                                    self.function_variant_name(FunctionTranslationStyle::Aborts),
                                    args_str,
                                );
                                emitln!(self.writer(), "$abort_flag := !$abort_if_cond;");
                            }
                        }

                        // regular path
                        if !processed {
                            let targeted = self.fun_target.module_env().is_target();
                            // If the callee has been generated from a native interface, return an error
                            if callee_env.is_native() && targeted {
                                for attr in callee_env.get_attributes() {
                                    if let Attribute::Apply(_, name, _) = attr {
                                        if self
                                            .fun_target
                                            .module_env()
                                            .symbol_pool()
                                            .string(*name)
                                            .as_str()
                                            == NATIVE_INTERFACE
                                        {
                                            let loc = self.fun_target.get_bytecode_loc(attr_id);
                                            self.parent
                                                .env
                                                .error(&loc, "Unknown native function is called");
                                        }
                                    }
                                }
                            }
                            let caller_mid = self.fun_target.module_env().get_id();
                            let caller_fid = self.fun_target.get_id();
                            let fun_verified =
                                !self.fun_target.func_env.is_explicitly_not_verified(
                                    &ProverOptions::get(self.fun_target.global_env()).verify_scope,
                                );
                            let mut fun_name = boogie_function_name(
                                &callee_env,
                                inst,
                                FunctionTranslationStyle::Default,
                            );

                            if self.parent.targets.get_fun_by_opaque_spec(
                                &self.fun_target.func_env.get_qualified_id(),
                            ) == Some(&QualifiedId {
                                module_id: *mid,
                                id: *fid,
                            }) {
                                if self.style == FunctionTranslationStyle::Default
                                    && self.fun_target.data.variant
                                        == FunctionVariant::Verification(
                                            VerificationFlavor::Regular,
                                        )
                                {
                                    fun_name = format!("{}{}", fun_name, "$impl");
                                } else if self.style == FunctionTranslationStyle::SpecNoAbortCheck
                                    || self.style == FunctionTranslationStyle::Opaque
                                {
                                    fun_name = format!("{}{}", fun_name, "$opaque");
                                }
                            };

                            // Helper function to check whether the idx corresponds to a bitwise operation
                            let compute_flag = |idx: TempIndex| {
                                targeted
                                    && fun_verified
                                    && *global_state
                                        .get_temp_index_oper(
                                            caller_mid,
                                            caller_fid,
                                            idx,
                                            baseline_flag,
                                        )
                                        .unwrap()
                                        == Bitwise
                            };
                            let instrument_bv2int =
                                |idx: TempIndex, args_str_vec: &mut Vec<String>| {
                                    let local_ty_srcs_1 = self.get_local_type(idx);
                                    let srcs_1_bv_flag = compute_flag(idx);
                                    let mut args_src_1_str = str_local(idx);
                                    if srcs_1_bv_flag {
                                        args_src_1_str = format!(
                                            "$bv2int.{}({})",
                                            boogie_num_type_base(&local_ty_srcs_1),
                                            args_src_1_str
                                        );
                                    }
                                    args_str_vec.push(args_src_1_str);
                                };
                            let callee_name = callee_env.get_name_str();
                            if dest_str.is_empty() {
                                let bv_flag = !srcs.is_empty() && compute_flag(srcs[0]);
                                if module_env.is_std_vector() {
                                    // Check the target vector contains bv values
                                    if callee_name.contains("insert") {
                                        let mut args_str_vec =
                                            vec![str_local(srcs[0]), str_local(srcs[1])];
                                        assert!(srcs.len() > 2);
                                        instrument_bv2int(srcs[2], &mut args_str_vec);
                                        args_str = args_str_vec.iter().cloned().join(", ");
                                    }
                                    fun_name =
                                        boogie_function_bv_name(&callee_env, inst, &[bv_flag]);
                                } else if module_env.is_table() {
                                    fun_name = boogie_function_bv_name(
                                        &callee_env,
                                        inst,
                                        &[false, bv_flag],
                                    );
                                }
                                emitln!(self.writer(), "call {}({});", fun_name, args_str);
                            } else {
                                let dest_bv_flag = !dests.is_empty() && compute_flag(dests[0]);
                                let bv_flag = !srcs.is_empty() && compute_flag(srcs[0]);
                                // Handle the case where the return value of length is assigned to a bv int because
                                // length always returns a non-bv result
                                if module_env.is_std_vector() {
                                    fun_name = boogie_function_bv_name(
                                        &callee_env,
                                        inst,
                                        &[bv_flag || dest_bv_flag],
                                    );
                                    // Handle the case where the return value of length is assigned to a bv int because
                                    // length always returns a non-bv result
                                    if callee_name.contains("length") && dest_bv_flag {
                                        let local_ty = self.get_local_type(dests[0]);
                                        // Insert '$' for calling function instead of procedure
                                        // TODO(tengzhang): a hacky way to convert int to bv for return value
                                        fun_name.insert(10, '$');
                                        // first call len fun then convert its return value to a bv type
                                        emitln!(
                                            self.writer(),
                                            "call {} := $int2bv{}({}({}));",
                                            dest_str,
                                            boogie_num_type_base(&local_ty),
                                            fun_name,
                                            args_str
                                        );
                                    } else if callee_name.contains("borrow")
                                        || callee_name.contains("remove")
                                        || callee_name.contains("swap")
                                    {
                                        let mut args_str_vec = vec![str_local(srcs[0])];
                                        instrument_bv2int(srcs[1], &mut args_str_vec);
                                        // Handle swap with three parameters
                                        if srcs.len() > 2 {
                                            instrument_bv2int(srcs[2], &mut args_str_vec);
                                        }
                                        args_str = args_str_vec.iter().cloned().join(", ");
                                    }
                                } else if module_env.is_table() {
                                    fun_name = boogie_function_bv_name(
                                        &callee_env,
                                        inst,
                                        &[false, bv_flag || dest_bv_flag],
                                    );
                                    if dest_bv_flag && callee_name.contains("length") {
                                        // Handle the case where the return value of length is assigned to a bv int because
                                        // length always returns a non-bv result
                                        let local_ty = self.get_local_type(dests[0]);
                                        // Replace with "spec_len"
                                        let length_idx_start = callee_name.find("length").unwrap();
                                        let length_idx_end = length_idx_start + "length".len();
                                        fun_name = [
                                            callee_name[0..length_idx_start].to_string(),
                                            "spec_len".to_string(),
                                            callee_name[length_idx_end..].to_string(),
                                        ]
                                        .join("");
                                        // first call len fun then convert its return value to a bv type
                                        emitln!(
                                            self.writer(),
                                            "call {} := $int2bv{}({}({}));",
                                            dest_str,
                                            boogie_num_type_base(&local_ty),
                                            fun_name,
                                            args_str
                                        );
                                    }
                                }
                                emitln!(
                                    self.writer(),
                                    "call {} := {}({});",
                                    dest_str,
                                    fun_name,
                                    args_str
                                );
                            }
                        }

                        if self
                            .parent
                            .targets
                            .get_fun_by_opaque_spec(&self.fun_target.func_env.get_qualified_id())
                            == Some(&mid.qualified(*fid))
                            && (self.style == FunctionTranslationStyle::SpecNoAbortCheck
                                || self.style == FunctionTranslationStyle::Opaque)
                        {
                            for type_inst in
                                spec_global_variable_analysis::get_info(&self.fun_target.data)
                                    .mut_vars()
                            {
                                emitln!(
                                    self.writer(),
                                    "havoc {};",
                                    boogie_spec_global_var_name(self.parent.env, type_inst),
                                );
                            }
                        };

                        // Clear the last track location after function call, as the call inserted
                        // location tracks before it returns.
                        *last_tracked_loc = None;
                    }
                    Pack(mid, sid, inst) => {
                        let inst = &self.inst_slice(inst);
                        let struct_env = env.get_module(*mid).into_struct(*sid);
                        let args = srcs.iter().cloned().map(str_local).join(", ");
                        let dest_str = str_local(dests[0]);
                        emitln!(
                            self.writer(),
                            "{} := {}({});",
                            dest_str,
                            boogie_struct_name(&struct_env, inst),
                            args
                        );
                    }
                    Unpack(mid, sid, inst) => {
                        let inst = &self.inst_slice(inst);
                        let struct_env = env.get_module(*mid).into_struct(*sid);
                        for (i, ref field_env) in struct_env.get_fields().enumerate() {
                            let field_sel = format!(
                                "{}->{}",
                                str_local(srcs[0]),
                                boogie_field_sel(field_env, inst),
                            );
                            emitln!(self.writer(), "{} := {};", str_local(dests[i]), field_sel);
                        }
                    }
                    BorrowField(mid, sid, inst, field_offset) => {
                        let inst = &self.inst_slice(inst);
                        let src_str = str_local(srcs[0]);
                        let dest_str = str_local(dests[0]);
                        let struct_env = env.get_module(*mid).into_struct(*sid);
                        let field_env = &struct_env.get_field_by_offset(*field_offset);
                        let sel_fun = boogie_field_sel(field_env, inst);
                        emitln!(
                            self.writer(),
                            "{} := $ChildMutation({}, {}, $Dereference({})->{});",
                            dest_str,
                            src_str,
                            field_offset,
                            src_str,
                            sel_fun
                        );
                    }
                    GetField(mid, sid, inst, field_offset) => {
                        let inst = &self.inst_slice(inst);
                        let src = srcs[0];
                        let mut src_str = str_local(src);
                        let dest_str = str_local(dests[0]);
                        let struct_env = env.get_module(*mid).into_struct(*sid);
                        let field_env = &struct_env.get_field_by_offset(*field_offset);
                        let sel_fun = boogie_field_sel(field_env, inst);
                        if self.get_local_type(src).is_reference() {
                            src_str = format!("$Dereference({})", src_str);
                        };
                        emitln!(self.writer(), "{} := {}->{};", dest_str, src_str, sel_fun);
                    }
                    Exists(mid, sid, inst) => {
                        let inst = self.inst_slice(inst);
                        let addr_str = str_local(srcs[0]);
                        let dest_str = str_local(dests[0]);
                        let memory = boogie_resource_memory_name(
                            env,
                            &mid.qualified_inst(*sid, inst),
                            &None,
                        );
                        emitln!(
                            self.writer(),
                            "{} := $ResourceExists({}, {});",
                            dest_str,
                            memory,
                            addr_str
                        );
                    }
                    BorrowGlobal(mid, sid, inst) => {
                        let inst = self.inst_slice(inst);
                        let addr_str = str_local(srcs[0]);
                        let dest_str = str_local(dests[0]);
                        let memory = boogie_resource_memory_name(
                            env,
                            &mid.qualified_inst(*sid, inst),
                            &None,
                        );
                        emitln!(
                            self.writer(),
                            "if (!$ResourceExists({}, {})) {{",
                            memory,
                            addr_str
                        );
                        self.writer()
                            .with_indent(|| emitln!(self.writer(), "call $ExecFailureAbort();"));
                        emitln!(self.writer(), "} else {");
                        self.writer().with_indent(|| {
                            emitln!(
                                self.writer(),
                                "{} := $Mutation($Global({}), EmptyVec(), $ResourceValue({}, {}));",
                                dest_str,
                                addr_str,
                                memory,
                                addr_str
                            );
                        });
                        emitln!(self.writer(), "}");
                    }
                    GetGlobal(mid, sid, inst) => {
                        let inst = self.inst_slice(inst);
                        let memory = boogie_resource_memory_name(
                            env,
                            &mid.qualified_inst(*sid, inst),
                            &None,
                        );
                        let addr_str = str_local(srcs[0]);
                        let dest_str = str_local(dests[0]);
                        emitln!(
                            self.writer(),
                            "if (!$ResourceExists({}, {})) {{",
                            memory,
                            addr_str
                        );
                        self.writer()
                            .with_indent(|| emitln!(self.writer(), "call $ExecFailureAbort();"));
                        emitln!(self.writer(), "} else {");
                        self.writer().with_indent(|| {
                            emitln!(
                                self.writer(),
                                "{} := $ResourceValue({}, {});",
                                dest_str,
                                memory,
                                addr_str
                            );
                        });
                        emitln!(self.writer(), "}");
                    }
                    MoveTo(mid, sid, inst) => {
                        let inst = self.inst_slice(inst);
                        let memory = boogie_resource_memory_name(
                            env,
                            &mid.qualified_inst(*sid, inst),
                            &None,
                        );
                        let value_str = str_local(srcs[0]);
                        let signer_str = str_local(srcs[1]);
                        emitln!(
                            self.writer(),
                            "if ($ResourceExists({}, {}->$addr)) {{",
                            memory,
                            signer_str
                        );
                        self.writer()
                            .with_indent(|| emitln!(self.writer(), "call $ExecFailureAbort();"));
                        emitln!(self.writer(), "} else {");
                        self.writer().with_indent(|| {
                            emitln!(
                                self.writer(),
                                "{} := $ResourceUpdate({}, {}->$addr, {});",
                                memory,
                                memory,
                                signer_str,
                                value_str
                            );
                        });
                        emitln!(self.writer(), "}");
                    }
                    MoveFrom(mid, sid, inst) => {
                        let inst = &self.inst_slice(inst);
                        let memory = boogie_resource_memory_name(
                            env,
                            &mid.qualified_inst(*sid, inst.to_owned()),
                            &None,
                        );
                        let addr_str = str_local(srcs[0]);
                        let dest_str = str_local(dests[0]);
                        emitln!(
                            self.writer(),
                            "if (!$ResourceExists({}, {})) {{",
                            memory,
                            addr_str
                        );
                        self.writer()
                            .with_indent(|| emitln!(self.writer(), "call $ExecFailureAbort();"));
                        emitln!(self.writer(), "} else {");
                        self.writer().with_indent(|| {
                            emitln!(
                                self.writer(),
                                "{} := $ResourceValue({}, {});",
                                dest_str,
                                memory,
                                addr_str
                            );
                            emitln!(
                                self.writer(),
                                "{} := $ResourceRemove({}, {});",
                                memory,
                                memory,
                                addr_str
                            );
                        });
                        emitln!(self.writer(), "}");
                    }
                    Havoc(HavocKind::Value) | Havoc(HavocKind::MutationAll) => {
                        let var_str = str_local(dests[0]);
                        emitln!(self.writer(), "havoc {};", var_str);
                    }
                    Havoc(HavocKind::MutationValue) => {
                        let ty = &self.get_local_type(dests[0]);
                        let num_oper = global_state
                            .get_temp_index_oper(mid, fid, dests[0], baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        let var_str = str_local(dests[0]);
                        let temp_str = boogie_temp(env, ty.skip_reference(), 0, bv_flag);
                        emitln!(self.writer(), "havoc {};", temp_str);
                        emitln!(
                            self.writer(),
                            "{} := $UpdateMutation({}, {});",
                            var_str,
                            var_str,
                            temp_str
                        );
                    }
                    Stop => {
                        // the two statements combined terminate any execution trace that reaches it
                        emitln!(self.writer(), "assume false;");
                        emitln!(self.writer(), "return;");
                    }
                    CastU8 | CastU16 | CastU32 | CastU64 | CastU128 | CastU256 => {
                        let src = srcs[0];
                        let dest = dests[0];
                        let make_cast = |target_base: &str, src: TempIndex, dest: TempIndex| {
                            let num_oper = global_state
                                .get_temp_index_oper(mid, fid, src, baseline_flag)
                                .unwrap();
                            let bv_flag = self.bv_flag(num_oper);
                            if bv_flag {
                                let src_type = self.get_local_type(src);
                                let base = boogie_num_type_base(&src_type);
                                emitln!(
                                    self.writer(),
                                    "call {} := $CastBv{}to{}({});",
                                    str_local(dest),
                                    base,
                                    target_base,
                                    str_local(src)
                                );
                            } else {
                                emitln!(
                                    self.writer(),
                                    "call {} := $CastU{}({});",
                                    str_local(dest),
                                    target_base,
                                    str_local(src)
                                );
                            }
                        };
                        let target_base = match oper {
                            CastU8 => "8",
                            CastU16 => "16",
                            CastU32 => "32",
                            CastU64 => "64",
                            CastU128 => "128",
                            CastU256 => "256",
                            _ => unreachable!(),
                        };
                        make_cast(target_base, src, dest);
                    }
                    Not => {
                        let src = srcs[0];
                        let dest = dests[0];
                        emitln!(
                            self.writer(),
                            "call {} := $Not({});",
                            str_local(dest),
                            str_local(src)
                        );
                    }
                    Add => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        let unchecked = if fun_target
                            .is_pragma_true(ADDITION_OVERFLOW_UNCHECKED_PRAGMA, || false)
                        {
                            "_unchecked"
                        } else {
                            ""
                        };
                        let num_oper = global_state
                            .get_temp_index_oper(mid, fid, dest, baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);

                        let add_type = match &self.get_local_type(dest) {
                            Type::Primitive(PrimitiveType::U8) => {
                                boogie_num_type_string_capital("8", bv_flag)
                            }
                            Type::Primitive(PrimitiveType::U16) => format!(
                                "{}{}",
                                boogie_num_type_string_capital("16", bv_flag),
                                unchecked
                            ),
                            Type::Primitive(PrimitiveType::U32) => format!(
                                "{}{}",
                                boogie_num_type_string_capital("32", bv_flag),
                                unchecked
                            ),
                            Type::Primitive(PrimitiveType::U64) => format!(
                                "{}{}",
                                boogie_num_type_string_capital("64", bv_flag),
                                unchecked
                            ),
                            Type::Primitive(PrimitiveType::U128) => format!(
                                "{}{}",
                                boogie_num_type_string_capital("128", bv_flag),
                                unchecked
                            ),
                            Type::Primitive(PrimitiveType::U256) => format!(
                                "{}{}",
                                boogie_num_type_string_capital("256", bv_flag),
                                unchecked
                            ),
                            Type::Primitive(_)
                            | Type::Tuple(_)
                            | Type::Vector(_)
                            | Type::Datatype(_, _, _)
                            | Type::TypeParameter(_)
                            | Type::Reference(_, _)
                            | Type::Fun(_, _)
                            | Type::TypeDomain(_)
                            | Type::ResourceDomain(_, _, _)
                            | Type::Error
                            | Type::Var(_) => unreachable!(),
                        };
                        emitln!(
                            self.writer(),
                            "call {} := $Add{}({}, {});",
                            str_local(dest),
                            add_type,
                            str_local(op1),
                            str_local(op2)
                        );
                    }
                    Sub => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        let num_oper = global_state
                            .get_temp_index_oper(mid, fid, dest, baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        if bv_flag {
                            let sub_type = match &self.get_local_type(dest) {
                                Type::Primitive(PrimitiveType::U8) => "Bv8".to_string(),
                                Type::Primitive(PrimitiveType::U16) => "Bv16".to_string(),
                                Type::Primitive(PrimitiveType::U32) => "Bv32".to_string(),
                                Type::Primitive(PrimitiveType::U64) => "Bv64".to_string(),
                                Type::Primitive(PrimitiveType::U128) => "Bv128".to_string(),
                                Type::Primitive(PrimitiveType::U256) => "Bv256".to_string(),
                                Type::Primitive(_)
                                | Type::Tuple(_)
                                | Type::Vector(_)
                                | Type::Datatype(_, _, _)
                                | Type::TypeParameter(_)
                                | Type::Reference(_, _)
                                | Type::Fun(_, _)
                                | Type::TypeDomain(_)
                                | Type::ResourceDomain(_, _, _)
                                | Type::Error
                                | Type::Var(_) => unreachable!(),
                            };
                            emitln!(
                                self.writer(),
                                "call {} := $Sub{}({}, {});",
                                str_local(dest),
                                sub_type,
                                str_local(op1),
                                str_local(op2)
                            );
                        } else {
                            emitln!(
                                self.writer(),
                                "call {} := $Sub({}, {});",
                                str_local(dest),
                                str_local(op1),
                                str_local(op2)
                            );
                        }
                    }
                    Mul => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        let num_oper = global_state
                            .get_temp_index_oper(mid, fid, dest, baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        let mul_type = match &self.get_local_type(dest) {
                            Type::Primitive(PrimitiveType::U8) => {
                                boogie_num_type_string_capital("8", bv_flag)
                            }
                            Type::Primitive(PrimitiveType::U16) => {
                                boogie_num_type_string_capital("16", bv_flag)
                            }
                            Type::Primitive(PrimitiveType::U32) => {
                                boogie_num_type_string_capital("32", bv_flag)
                            }
                            Type::Primitive(PrimitiveType::U64) => {
                                boogie_num_type_string_capital("64", bv_flag)
                            }
                            Type::Primitive(PrimitiveType::U128) => {
                                boogie_num_type_string_capital("128", bv_flag)
                            }
                            Type::Primitive(PrimitiveType::U256) => {
                                boogie_num_type_string_capital("256", bv_flag)
                            }
                            Type::Primitive(_)
                            | Type::Tuple(_)
                            | Type::Vector(_)
                            | Type::Datatype(_, _, _)
                            | Type::TypeParameter(_)
                            | Type::Reference(_, _)
                            | Type::Fun(_, _)
                            | Type::TypeDomain(_)
                            | Type::ResourceDomain(_, _, _)
                            | Type::Error
                            | Type::Var(_) => unreachable!(),
                        };
                        emitln!(
                            self.writer(),
                            "call {} := $Mul{}({}, {});",
                            str_local(dest),
                            mul_type,
                            str_local(op1),
                            str_local(op2)
                        );
                    }
                    Div => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        let num_oper = global_state
                            .get_temp_index_oper(mid, fid, dest, baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        let div_type = if bv_flag {
                            match &self.get_local_type(dest) {
                                Type::Primitive(PrimitiveType::U8) => "Bv8".to_string(),
                                Type::Primitive(PrimitiveType::U16) => "Bv16".to_string(),
                                Type::Primitive(PrimitiveType::U32) => "Bv32".to_string(),
                                Type::Primitive(PrimitiveType::U64) => "Bv64".to_string(),
                                Type::Primitive(PrimitiveType::U128) => "Bv128".to_string(),
                                Type::Primitive(PrimitiveType::U256) => "Bv256".to_string(),
                                Type::Primitive(_)
                                | Type::Tuple(_)
                                | Type::Vector(_)
                                | Type::Datatype(_, _, _)
                                | Type::TypeParameter(_)
                                | Type::Reference(_, _)
                                | Type::Fun(_, _)
                                | Type::TypeDomain(_)
                                | Type::ResourceDomain(_, _, _)
                                | Type::Error
                                | Type::Var(_) => unreachable!(),
                            }
                        } else {
                            "".to_string()
                        };
                        emitln!(
                            self.writer(),
                            "call {} := $Div{}({}, {});",
                            str_local(dest),
                            div_type,
                            str_local(op1),
                            str_local(op2)
                        );
                    }
                    Mod => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        let num_oper = global_state
                            .get_temp_index_oper(mid, fid, dest, baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        let mod_type = if bv_flag {
                            match &self.get_local_type(dest) {
                                Type::Primitive(PrimitiveType::U8) => "Bv8".to_string(),
                                Type::Primitive(PrimitiveType::U16) => "Bv16".to_string(),
                                Type::Primitive(PrimitiveType::U32) => "Bv32".to_string(),
                                Type::Primitive(PrimitiveType::U64) => "Bv64".to_string(),
                                Type::Primitive(PrimitiveType::U128) => "Bv128".to_string(),
                                Type::Primitive(PrimitiveType::U256) => "Bv256".to_string(),
                                Type::Primitive(_)
                                | Type::Tuple(_)
                                | Type::Vector(_)
                                | Type::Datatype(_, _, _)
                                | Type::TypeParameter(_)
                                | Type::Reference(_, _)
                                | Type::Fun(_, _)
                                | Type::TypeDomain(_)
                                | Type::ResourceDomain(_, _, _)
                                | Type::Error
                                | Type::Var(_) => unreachable!(),
                            }
                        } else {
                            "".to_string()
                        };
                        emitln!(
                            self.writer(),
                            "call {} := $Mod{}({}, {});",
                            str_local(dest),
                            mod_type,
                            str_local(op1),
                            str_local(op2)
                        );
                    }
                    Shl | Shr => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        let sh_oper_str = if oper == &Shl { "Shl" } else { "Shr" };
                        let num_oper = global_state
                            .get_temp_index_oper(mid, fid, dest, baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        if bv_flag {
                            let target_type = match &self.get_local_type(dest) {
                                Type::Primitive(PrimitiveType::U8) => "Bv8",
                                Type::Primitive(PrimitiveType::U16) => "Bv16",
                                Type::Primitive(PrimitiveType::U32) => "Bv32",
                                Type::Primitive(PrimitiveType::U64) => "Bv64",
                                Type::Primitive(PrimitiveType::U128) => "Bv128",
                                Type::Primitive(PrimitiveType::U256) => "Bv256",
                                Type::Primitive(_)
                                | Type::Tuple(_)
                                | Type::Vector(_)
                                | Type::Datatype(_, _, _)
                                | Type::TypeParameter(_)
                                | Type::Reference(_, _)
                                | Type::Fun(_, _)
                                | Type::TypeDomain(_)
                                | Type::ResourceDomain(_, _, _)
                                | Type::Error
                                | Type::Var(_) => unreachable!(),
                            };
                            let src_type = boogie_num_type_base(&self.get_local_type(op2));
                            emitln!(
                                self.writer(),
                                "call {} := ${}Bv{}From{}({}, {});",
                                str_local(dest),
                                sh_oper_str,
                                target_type,
                                src_type,
                                str_local(op1),
                                str_local(op2)
                            );
                        } else {
                            let sh_type = match &self.get_local_type(dest) {
                                Type::Primitive(PrimitiveType::U8) => "U8",
                                Type::Primitive(PrimitiveType::U16) => "U16",
                                Type::Primitive(PrimitiveType::U32) => "U32",
                                Type::Primitive(PrimitiveType::U64) => "U64",
                                Type::Primitive(PrimitiveType::U128) => "U128",
                                Type::Primitive(PrimitiveType::U256) => "U256",
                                Type::Primitive(_)
                                | Type::Tuple(_)
                                | Type::Vector(_)
                                | Type::Datatype(_, _, _)
                                | Type::TypeParameter(_)
                                | Type::Reference(_, _)
                                | Type::Fun(_, _)
                                | Type::TypeDomain(_)
                                | Type::ResourceDomain(_, _, _)
                                | Type::Error
                                | Type::Var(_) => unreachable!(),
                            };
                            emitln!(
                                self.writer(),
                                "call {} := ${}{}({}, {});",
                                str_local(dest),
                                sh_oper_str,
                                sh_type,
                                str_local(op1),
                                str_local(op2)
                            );
                        }
                    }
                    Lt | Le | Gt | Ge => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        let make_comparison = |comp_oper: &str, op1, op2, dest| {
                            let num_oper = global_state
                                .get_temp_index_oper(mid, fid, op1, baseline_flag)
                                .unwrap();
                            let bv_flag = self.bv_flag(num_oper);
                            let lt_type = if bv_flag {
                                match &self.get_local_type(op1) {
                                    Type::Primitive(PrimitiveType::U8) => "Bv8".to_string(),
                                    Type::Primitive(PrimitiveType::U16) => "Bv16".to_string(),
                                    Type::Primitive(PrimitiveType::U32) => "Bv32".to_string(),
                                    Type::Primitive(PrimitiveType::U64) => "Bv64".to_string(),
                                    Type::Primitive(PrimitiveType::U128) => "Bv128".to_string(),
                                    Type::Primitive(PrimitiveType::U256) => "Bv256".to_string(),
                                    Type::Primitive(_)
                                    | Type::Tuple(_)
                                    | Type::Vector(_)
                                    | Type::Datatype(_, _, _)
                                    | Type::TypeParameter(_)
                                    | Type::Reference(_, _)
                                    | Type::Fun(_, _)
                                    | Type::TypeDomain(_)
                                    | Type::ResourceDomain(_, _, _)
                                    | Type::Error
                                    | Type::Var(_) => unreachable!(),
                                }
                            } else {
                                "".to_string()
                            };
                            emitln!(
                                self.writer(),
                                "call {} := {}{}({}, {});",
                                str_local(dest),
                                comp_oper,
                                lt_type,
                                str_local(op1),
                                str_local(op2)
                            );
                        };
                        let comp_oper = match oper {
                            Lt => "$Lt",
                            Le => "$Le",
                            Gt => "$Gt",
                            Ge => "$Ge",
                            _ => unreachable!(),
                        };
                        make_comparison(comp_oper, op1, op2, dest);
                    }
                    Or => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        emitln!(
                            self.writer(),
                            "call {} := $Or({}, {});",
                            str_local(dest),
                            str_local(op1),
                            str_local(op2)
                        );
                    }
                    And => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        emitln!(
                            self.writer(),
                            "call {} := $And({}, {});",
                            str_local(dest),
                            str_local(op1),
                            str_local(op2)
                        );
                    }
                    Eq | Neq => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        let num_oper = global_state
                            .get_temp_index_oper(mid, fid, op1, baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        let oper = boogie_equality_for_type(
                            env,
                            oper == &Eq,
                            &self.get_local_type(op1),
                            bv_flag,
                        );
                        emitln!(
                            self.writer(),
                            "{} := {}({}, {});",
                            str_local(dest),
                            oper,
                            str_local(op1),
                            str_local(op2)
                        );
                    }
                    Xor | BitOr | BitAnd => {
                        let dest = dests[0];
                        let op1 = srcs[0];
                        let op2 = srcs[1];
                        let make_bitwise =
                            |bv_oper: &str, op1: TempIndex, op2: TempIndex, dest: TempIndex| {
                                let base = match &self.get_local_type(dest) {
                                    Type::Primitive(PrimitiveType::U8) => "Bv8".to_string(),
                                    Type::Primitive(PrimitiveType::U16) => "Bv16".to_string(),
                                    Type::Primitive(PrimitiveType::U32) => "Bv32".to_string(),
                                    Type::Primitive(PrimitiveType::U64) => "Bv64".to_string(),
                                    Type::Primitive(PrimitiveType::U128) => "Bv128".to_string(),
                                    Type::Primitive(PrimitiveType::U256) => "Bv256".to_string(),
                                    Type::Primitive(_)
                                    | Type::Tuple(_)
                                    | Type::Vector(_)
                                    | Type::Datatype(_, _, _)
                                    | Type::TypeParameter(_)
                                    | Type::Reference(_, _)
                                    | Type::Fun(_, _)
                                    | Type::TypeDomain(_)
                                    | Type::ResourceDomain(_, _, _)
                                    | Type::Error
                                    | Type::Var(_) => unreachable!(),
                                };
                                let op1_ty = &self.get_local_type(op1);
                                let op2_ty = &self.get_local_type(op2);
                                let num_oper_1 = global_state
                                    .get_temp_index_oper(mid, fid, op1, baseline_flag)
                                    .unwrap();
                                let op1_bv_flag = self.bv_flag(num_oper_1);
                                let num_oper_2 = global_state
                                    .get_temp_index_oper(mid, fid, op2, baseline_flag)
                                    .unwrap();
                                let op2_bv_flag = self.bv_flag(num_oper_2);
                                let op1_str = if !op1_bv_flag {
                                    format!(
                                        "$int2bv.{}({})",
                                        boogie_num_type_base(op1_ty),
                                        str_local(op1)
                                    )
                                } else {
                                    str_local(op1)
                                };
                                let op2_str = if !op2_bv_flag {
                                    format!(
                                        "$int2bv.{}({})",
                                        boogie_num_type_base(op2_ty),
                                        str_local(op2)
                                    )
                                } else {
                                    str_local(op2)
                                };
                                emitln!(
                                    self.writer(),
                                    "call {} := {}{}({}, {});",
                                    str_local(dest),
                                    bv_oper,
                                    base,
                                    op1_str,
                                    op2_str
                                );
                            };
                        let bv_oper_str = match oper {
                            Xor => "$Xor",
                            BitOr => "$Or",
                            BitAnd => "$And",
                            _ => unreachable!(),
                        };
                        make_bitwise(bv_oper_str, op1, op2, dest);
                    }
                    Uninit => {
                        emitln!(
                            self.writer(),
                            "assume $t{}->l == $Uninitialized();",
                            srcs[0]
                        );
                    }
                    Destroy => {}
                    TraceLocal(idx) => {
                        let num_oper = global_state
                            .get_temp_index_oper(mid, fid, srcs[0], baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        self.track_local(*idx, srcs[0], bv_flag);
                    }
                    TraceReturn(i) => {
                        let oper_map = global_state.get_ret_map();
                        let bv_flag = self.bv_flag_from_map(&srcs[0], oper_map);
                        self.track_return(*i, srcs[0], bv_flag);
                    }
                    TraceAbort => self.track_abort(&str_local(srcs[0])),
                    TraceExp(kind, node_id) => {
                        let bv_flag = *global_state
                            .get_temp_index_oper(mid, fid, srcs[0], baseline_flag)
                            .unwrap()
                            == Bitwise;
                        self.track_exp(*kind, *node_id, srcs[0], bv_flag)
                    }
                    EmitEvent => {
                        let msg = srcs[0];
                        let handle = srcs[1];
                        let suffix = boogie_type_suffix(env, &self.get_local_type(msg));
                        emit!(
                            self.writer(),
                            "$es := ${}ExtendEventStore'{}'($es, ",
                            if srcs.len() > 2 { "Cond" } else { "" },
                            suffix
                        );
                        emit!(self.writer(), "{}, {}", str_local(handle), str_local(msg));
                        if srcs.len() > 2 {
                            emit!(self.writer(), ", {}", str_local(srcs[2]));
                        }
                        emitln!(self.writer(), ");");
                    }
                    EventStoreDiverge => {
                        emitln!(self.writer(), "call $es := $EventStore__diverge($es);");
                    }
                    TraceGlobalMem(mem) => {
                        let mem = &mem.to_owned().instantiate(self.type_inst);
                        let node_id = env.new_node(env.unknown_loc(), mem.to_type());
                        self.track_global_mem(mem, node_id);
                    }
                }
                match aa {
                    Some(AbortAction::Jump(target, code)) => {
                        emitln!(self.writer(), "if ($abort_flag) {");
                        self.writer().indent();
                        *last_tracked_loc = None;
                        self.track_loc(last_tracked_loc, &loc);
                        let code_str = str_local(*code);
                        emitln!(self.writer(), "{} := $abort_code;", code_str);
                        self.track_abort(&code_str);
                        emitln!(self.writer(), "goto L{};", target.as_usize());
                        self.writer().unindent();
                        emitln!(self.writer(), "}");
                    }
                    Some(AbortAction::Check) => {
                        emitln!(self.writer(), "assert {:msg \"assert_failed(0,0,0): code should not abort\"} !$abort_flag;");
                    }
                    None => {}
                }
            }
            Abort(_, src) => {
                if FunctionTranslationStyle::Default == self.style
                    && self.fun_target.data.variant
                        == FunctionVariant::Verification(VerificationFlavor::Regular)
                    && self
                        .parent
                        .targets
                        .get_fun_by_opaque_spec(&self.fun_target.func_env.get_qualified_id())
                        .is_some()
                    && !self
                        .parent
                        .targets
                        .no_asserts()
                        .contains(&self.fun_target.func_env.get_qualified_id())
                {
                    emitln!(self.writer(), "$abort_flag := false;");
                    emitln!(
                        self.writer(),
                        "call $abort_if_cond := {}({});",
                        self.function_variant_name(FunctionTranslationStyle::Aborts),
                        (0..fun_target.get_parameter_count())
                            .map(|i| {
                                let prefix = if self.parameter_needs_to_be_mutable(fun_target, i) {
                                    "_$"
                                } else {
                                    "$"
                                };
                                format!("{}t{}", prefix, i)
                            })
                            .join(", "),
                    );
                    emitln!(self.writer(), "assert {:msg \"assert_failed(0,0,0): abort_if assertion does not hold\"} !$abort_if_cond;");
                }
                emitln!(self.writer(), "$abort_code := {};", str_local(*src));
                emitln!(self.writer(), "$abort_flag := true;");
                emitln!(self.writer(), "return;")
            }
            Nop(..) => {}
        }
        emitln!(self.writer());
    }

    fn translate_write_back(&self, dest: &BorrowNode, edge: &BorrowEdge, src: TempIndex) {
        use BorrowNode::*;
        let writer = self.parent.writer;
        let env = self.parent.env;
        let src_str = format!("$t{}", src);
        match dest {
            ReturnPlaceholder(_) => {
                unreachable!("unexpected transient borrow node")
            }
            GlobalRoot(memory) => {
                assert!(matches!(edge, BorrowEdge::Direct));
                let memory = &memory.to_owned().instantiate(self.type_inst);
                let memory_name = boogie_resource_memory_name(env, memory, &None);
                emitln!(
                    writer,
                    "{} := $ResourceUpdate({}, $GlobalLocationAddress({}),\n    \
                     $Dereference({}));",
                    memory_name,
                    memory_name,
                    src_str,
                    src_str
                );
            }
            LocalRoot(idx) => {
                assert!(matches!(edge, BorrowEdge::Direct));
                emitln!(writer, "$t{} := $Dereference({});", idx, src_str);
            }
            Reference(idx) => {
                let dst_value = format!("$Dereference($t{})", idx);
                let src_value = format!("$Dereference({})", src_str);
                let get_path_index = |offset: usize| {
                    if offset == 0 {
                        format!("ReadVec({}->p, LenVec($t{}->p))", src_str, idx)
                    } else {
                        format!("ReadVec({}->p, LenVec($t{}->p) + {})", src_str, idx, offset)
                    }
                };
                let update = if let BorrowEdge::Hyper(edges) = edge {
                    self.translate_write_back_update(
                        &mut || dst_value.clone(),
                        &get_path_index,
                        src_value,
                        edges,
                        0,
                    )
                } else {
                    self.translate_write_back_update(
                        &mut || dst_value.clone(),
                        &get_path_index,
                        src_value,
                        &[edge.to_owned()],
                        0,
                    )
                };
                emitln!(
                    writer,
                    "$t{} := $UpdateMutation($t{}, {});",
                    idx,
                    idx,
                    update
                );
            }
        }
    }

    /// Returns read aggregate and write aggregate if fun_env matches one of the native functions
    /// implementing custom mutable borrow.
    fn get_borrow_native_aggregate_names(&self, fn_name: &String) -> Option<(String, String)> {
        for f in &self.parent.options.borrow_aggregates {
            if &f.name == fn_name {
                return Some((f.read_aggregate.clone(), f.write_aggregate.clone()));
            }
        }
        None
    }

    fn translate_write_back_update(
        &self,
        mk_dest: &mut dyn FnMut() -> String,
        get_path_index: &dyn Fn(usize) -> String,
        src: String,
        edges: &[BorrowEdge],
        at: usize,
    ) -> String {
        if at >= edges.len() {
            src
        } else {
            match &edges[at] {
                BorrowEdge::Direct => {
                    self.translate_write_back_update(mk_dest, get_path_index, src, edges, at + 1)
                }
                BorrowEdge::Field(memory, offset) => {
                    let memory = memory.to_owned().instantiate(self.type_inst);
                    let struct_env = &self.parent.env.get_struct_qid(memory.to_qualified_id());
                    let field_env = &struct_env.get_field_by_offset(*offset);
                    let sel_fun = boogie_field_sel(field_env, &memory.inst);
                    let new_dest = format!("{}->{}", (*mk_dest)(), sel_fun);
                    let mut new_dest_needed = false;
                    let new_src = self.translate_write_back_update(
                        &mut || {
                            new_dest_needed = true;
                            format!("$$sel{}", at)
                        },
                        get_path_index,
                        src,
                        edges,
                        at + 1,
                    );
                    let update_fun = boogie_field_update(field_env, &memory.inst);
                    if new_dest_needed {
                        format!(
                            "(var $$sel{} := {}; {}({}, {}))",
                            at,
                            new_dest,
                            update_fun,
                            (*mk_dest)(),
                            new_src
                        )
                    } else {
                        format!("{}({}, {})", update_fun, (*mk_dest)(), new_src)
                    }
                }
                BorrowEdge::Index(index_edge_kind) => {
                    // Index edge is used for both vectors, tables, and custom native methods
                    // implementing similar functionality (mutable borrow). Determine which
                    // operations to use to read and update.
                    let (read_aggregate, update_aggregate) = match index_edge_kind {
                        IndexEdgeKind::Vector => ("ReadVec".to_string(), "UpdateVec".to_string()),
                        IndexEdgeKind::Table => ("GetTable".to_string(), "UpdateTable".to_string()),
                        IndexEdgeKind::Custom(name) => {
                            // panic here means that custom borrow natives options were not specified properly
                            self.get_borrow_native_aggregate_names(name).unwrap()
                        }
                    };

                    // Compute the offset into the path where to retrieve the index.
                    let offset = edges[0..at]
                        .iter()
                        .filter(|e| !matches!(e, BorrowEdge::Direct))
                        .count();
                    let index = (*get_path_index)(offset);
                    let new_dest = format!("{}({}, {})", read_aggregate, (*mk_dest)(), index);
                    let mut new_dest_needed = false;
                    // Recursively perform write backs for next edges
                    let new_src = self.translate_write_back_update(
                        &mut || {
                            new_dest_needed = true;
                            format!("$$sel{}", at)
                        },
                        get_path_index,
                        src,
                        edges,
                        at + 1,
                    );
                    if new_dest_needed {
                        format!(
                            "(var $$sel{} := {}; {}({}, {}, {}))",
                            at,
                            new_dest,
                            update_aggregate,
                            (*mk_dest)(),
                            index,
                            new_src
                        )
                    } else {
                        format!(
                            "{}({}, {}, {})",
                            update_aggregate,
                            (*mk_dest)(),
                            index,
                            new_src
                        )
                    }
                }
                BorrowEdge::Hyper(_) => unreachable!("unexpected borrow edge"),
            }
        }
    }

    /// Track location for execution trace, avoiding to track the same line multiple times.
    fn track_loc(&self, last_tracked_loc: &mut Option<(Loc, LineIndex)>, loc: &Loc) {
        let env = self.fun_target.global_env();
        if let Some(l) = env.get_location(loc) {
            if let Some((last_loc, last_line)) = last_tracked_loc {
                if *last_line == l.line {
                    // This line already tracked.
                    return;
                }
                *last_loc = loc.clone();
                *last_line = l.line;
            } else {
                *last_tracked_loc = Some((loc.clone(), l.line));
            }
            emitln!(
                self.writer(),
                "assume {{:print \"$at{}\"}} true;",
                self.loc_str(loc)
            );
        }
    }

    fn track_abort(&self, code_var: &str) {
        emitln!(
            self.writer(),
            &boogie_debug_track_abort(self.fun_target, code_var)
        );
    }

    /// Generates an update of the debug information about temporary.
    fn track_local(&self, origin_idx: TempIndex, idx: TempIndex, bv_flag: bool) {
        emitln!(
            self.writer(),
            &boogie_debug_track_local(
                self.fun_target,
                origin_idx,
                idx,
                &self.get_local_type(idx),
                bv_flag
            )
        );
    }

    /// Generates an update of the debug information about the return value at given location.
    fn track_return(&self, return_idx: usize, idx: TempIndex, bv_flag: bool) {
        emitln!(
            self.writer(),
            &boogie_debug_track_return(
                self.fun_target,
                return_idx,
                idx,
                &self.get_local_type(idx),
                bv_flag
            )
        );
    }

    /// Generates the bytecode to print out the value of mem.
    fn track_global_mem(&self, mem: &QualifiedInstId<DatatypeId>, node_id: NodeId) {
        let env = self.parent.env;
        let temp_str = boogie_resource_memory_name(env, mem, &None);
        emitln!(
            self.writer(),
            "assume {{:print \"$track_global_mem({}):\", {}}} true;",
            node_id.as_usize(),
            temp_str,
        );
    }

    fn track_exp(&self, kind: TraceKind, node_id: NodeId, temp: TempIndex, bv_flag: bool) {
        let env = self.parent.env;
        let ty = self.get_local_type(temp);
        let temp_str = if ty.is_reference() {
            let new_temp = boogie_temp(env, ty.skip_reference(), 0, bv_flag);
            emitln!(self.writer(), "{} := $Dereference($t{});", new_temp, temp);
            new_temp
        } else {
            format!("$t{}", temp)
        };
        let suffix = if kind == TraceKind::SubAuto {
            "_sub"
        } else {
            ""
        };
        emitln!(
            self.writer(),
            "assume {{:print \"$track_exp{}({}):\", {}}} true;",
            suffix,
            node_id.as_usize(),
            temp_str,
        );
    }

    fn loc_str(&self, loc: &Loc) -> String {
        let file_idx = self.fun_target.global_env().file_id_to_idx(loc.file_id());
        format!("({},{},{})", file_idx, loc.span().start(), loc.span().end())
    }

    fn compute_needed_temps(&self) -> BTreeMap<(String, bool), (Type, bool, usize)> {
        use Bytecode::*;
        use Operation::*;

        let fun_target = self.fun_target;
        let env = fun_target.global_env();

        let mut res: BTreeMap<(String, bool), (Type, bool, usize)> = BTreeMap::new();
        let mut need = |ty: &Type, bv_flag: bool, n: usize| {
            // Index by type suffix, which is more coarse grained then type.
            let ty = ty.skip_reference();
            let suffix = boogie_type_suffix(env, ty);
            let cnt = res
                .entry((suffix, bv_flag))
                .or_insert_with(|| (ty.to_owned(), bv_flag, 0));
            cnt.2 = cnt.2.max(n);
        };
        let baseline_flag = self.fun_target.data.variant == FunctionVariant::Baseline;
        let global_state = &self
            .fun_target
            .global_env()
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let ret_oper_map = &global_state.get_ret_map();
        let mid = fun_target.func_env.module_env.get_id();
        let fid = fun_target.func_env.get_id();

        for bc in &fun_target.data.code {
            match bc {
                Call(_, dests, oper, srcs, ..) => match oper {
                    TraceExp(_, id) => {
                        let ty = &self.inst(&env.get_node_type(*id));
                        let bv_flag = global_state.get_node_num_oper(*id) == Bitwise;
                        need(ty, bv_flag, 1)
                    }
                    TraceReturn(idx) => {
                        let ty = &self.inst(fun_target.get_return_type(*idx));
                        let bv_flag = self.bv_flag_from_map(idx, ret_oper_map);
                        need(ty, bv_flag, 1)
                    }
                    TraceLocal(_) => {
                        let ty = &self.get_local_type(srcs[0]);
                        let num_oper = &global_state
                            .get_temp_index_oper(mid, fid, srcs[0], baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        need(ty, bv_flag, 1)
                    }
                    Havoc(HavocKind::MutationValue) => {
                        let ty = &self.get_local_type(dests[0]);
                        let num_oper = &global_state
                            .get_temp_index_oper(mid, fid, dests[0], baseline_flag)
                            .unwrap();
                        let bv_flag = self.bv_flag(num_oper);
                        need(ty, bv_flag, 1)
                    }
                    _ => {}
                },
                Prop(_, PropKind::Modifies, exp) => {
                    // global_state.exp_operation_map.get(exp.node_id()) == Bitwise;
                    //let bv_flag = env.get_node_num_oper(exp.node_id()) == Bitwise;
                    let bv_flag = global_state.get_node_num_oper(exp.node_id()) == Bitwise;
                    need(&BOOL_TYPE, false, 1);
                    need(&self.inst(&env.get_node_type(exp.node_id())), bv_flag, 1)
                }
                _ => {}
            }
        }
        res
    }
}

fn struct_has_native_equality(
    struct_env: &StructEnv<'_>,
    inst: &[Type],
    options: &BoogieOptions,
) -> bool {
    if options.native_equality {
        // Everything has native equality
        return true;
    }
    for field in struct_env.get_fields() {
        if !has_native_equality(
            struct_env.module_env.env,
            options,
            &field.get_type().instantiate(inst),
        ) {
            return false;
        }
    }
    true
}

pub fn has_native_equality(env: &GlobalEnv, options: &BoogieOptions, ty: &Type) -> bool {
    if options.native_equality {
        // Everything has native equality
        return true;
    }
    match ty {
        Type::Vector(..) => false,
        Type::Datatype(mid, sid, sinst) => {
            struct_has_native_equality(&env.get_struct_qid(mid.qualified(*sid)), sinst, options)
        }
        Type::Primitive(_)
        | Type::Tuple(_)
        | Type::TypeParameter(_)
        | Type::Reference(_, _)
        | Type::Fun(_, _)
        | Type::TypeDomain(_)
        | Type::ResourceDomain(_, _, _)
        | Type::Error
        | Type::Var(_) => true,
    }
}
