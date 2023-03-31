// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use itertools::Itertools;
#[allow(unused_imports)]
use log::{debug, info, warn};
use num::BigUint;
use serde::{Deserialize, Serialize};
use tera::{Context, Tera};

use move_model::{
    code_writer::CodeWriter,
    emit, emitln,
    model::{GlobalEnv, QualifiedId, StructId},
    pragmas::{
        INTRINSIC_FUN_MAP_ADD_NO_OVERRIDE, INTRINSIC_FUN_MAP_ADD_OVERRIDE_IF_EXISTS,
        INTRINSIC_FUN_MAP_BORROW, INTRINSIC_FUN_MAP_BORROW_MUT, INTRINSIC_FUN_MAP_DEL_MUST_EXIST,
        INTRINSIC_FUN_MAP_DEL_RETURN_KEY, INTRINSIC_FUN_MAP_DESTROY_EMPTY,
        INTRINSIC_FUN_MAP_HAS_KEY, INTRINSIC_FUN_MAP_IS_EMPTY, INTRINSIC_FUN_MAP_LEN,
        INTRINSIC_FUN_MAP_NEW, INTRINSIC_FUN_MAP_SPEC_DEL, INTRINSIC_FUN_MAP_SPEC_GET,
        INTRINSIC_FUN_MAP_SPEC_HAS_KEY, INTRINSIC_FUN_MAP_SPEC_IS_EMPTY,
        INTRINSIC_FUN_MAP_SPEC_LEN, INTRINSIC_FUN_MAP_SPEC_NEW, INTRINSIC_FUN_MAP_SPEC_SET,
    },
    ty::{PrimitiveType, Type},
};
use move_stackless_bytecode::mono_analysis;

use crate::{
    boogie_helpers::{boogie_bv_type, boogie_module_name, boogie_type, boogie_type_suffix_bv},
    bytecode_translator::has_native_equality,
    options::{BoogieOptions, VectorTheory},
};

const PRELUDE_TEMPLATE: &[u8] = include_bytes!("prelude/prelude.bpl");
const NATIVE_TEMPLATE: &[u8] = include_bytes!("prelude/native.bpl");
const VECTOR_ARRAY_THEORY: &[u8] = include_bytes!("prelude/vector-array-theory.bpl");
const VECTOR_ARRAY_INTERN_THEORY: &[u8] = include_bytes!("prelude/vector-array-intern-theory.bpl");
const VECTOR_SMT_SEQ_THEORY: &[u8] = include_bytes!("prelude/vector-smt-seq-theory.bpl");
const VECTOR_SMT_ARRAY_THEORY: &[u8] = include_bytes!("prelude/vector-smt-array-theory.bpl");
const VECTOR_SMT_ARRAY_EXT_THEORY: &[u8] =
    include_bytes!("prelude/vector-smt-array-ext-theory.bpl");
const MULTISET_ARRAY_THEORY: &[u8] = include_bytes!("prelude/multiset-array-theory.bpl");
const TABLE_ARRAY_THEORY: &[u8] = include_bytes!("prelude/table-array-theory.bpl");

// TODO use named addresses
const BCS_MODULE: &str = "0x1::bcs";
const EVENT_MODULE: &str = "0x1::event";

mod boogie_helpers;
pub mod boogie_wrapper;
pub mod bytecode_translator;
pub mod options;
mod prover_task_runner;
mod spec_translator;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
struct TypeInfo {
    name: String,
    suffix: String,
    has_native_equality: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
struct BvInfo {
    base: usize,
    max: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
struct MapImpl {
    struct_name: String,
    insts: Vec<(TypeInfo, TypeInfo)>,
    // move functions
    fun_new: String,
    fun_destroy_empty: String,
    fun_len: String,
    fun_is_empty: String,
    fun_has_key: String,
    fun_add_no_override: String,
    fun_add_override_if_exists: String,
    fun_del_must_exist: String,
    fun_del_return_key: String,
    fun_borrow: String,
    fun_borrow_mut: String,
    // spec functions
    fun_spec_new: String,
    fun_spec_get: String,
    fun_spec_set: String,
    fun_spec_del: String,
    fun_spec_len: String,
    fun_spec_is_empty: String,
    fun_spec_has_key: String,
}

/// Help generating vector functions for bv types
fn bv_helper() -> Vec<BvInfo> {
    let mut bv_info = vec![];
    let bv_8 = BvInfo {
        base: 8,
        max: "255".to_string(),
    };
    bv_info.push(bv_8);
    let bv_16 = BvInfo {
        base: 16,
        max: "65535".to_string(),
    };
    bv_info.push(bv_16);
    let bv_32 = BvInfo {
        base: 32,
        max: "2147483647".to_string(),
    };
    bv_info.push(bv_32);
    let bv_64 = BvInfo {
        base: 64,
        max: "18446744073709551615".to_string(),
    };
    bv_info.push(bv_64);
    let bv_128 = BvInfo {
        base: 128,
        max: "340282366920938463463374607431768211455".to_string(),
    };
    bv_info.push(bv_128);
    let bv_256 = BvInfo {
        base: 256,
        max: "115792089237316195423570985008687907853269984665640564039457584007913129639935"
            .to_string(),
    };
    bv_info.push(bv_256);
    bv_info
}

/// Adds the prelude to the generated output.
pub fn add_prelude(
    env: &GlobalEnv,
    options: &BoogieOptions,
    writer: &CodeWriter,
) -> anyhow::Result<()> {
    emit!(writer, "\n// ** Expanded prelude\n\n");
    let templ = |name: &'static str, cont: &[u8]| (name, String::from_utf8_lossy(cont).to_string());

    // Add the prelude template.
    let mut templates = vec![
        templ("native", NATIVE_TEMPLATE),
        templ("prelude", PRELUDE_TEMPLATE),
        // Add the basic array theory to make it available for inclusion in other theories.
        templ("vector-array-theory", VECTOR_ARRAY_THEORY),
    ];

    // Bind the chosen vector and multiset theory
    let vector_theory = match options.vector_theory {
        VectorTheory::BoogieArray => VECTOR_ARRAY_THEORY,
        VectorTheory::BoogieArrayIntern => VECTOR_ARRAY_INTERN_THEORY,
        VectorTheory::SmtArray => VECTOR_SMT_ARRAY_THEORY,
        VectorTheory::SmtArrayExt => VECTOR_SMT_ARRAY_EXT_THEORY,
        VectorTheory::SmtSeq => VECTOR_SMT_SEQ_THEORY,
    };
    templates.push(templ("vector-theory", vector_theory));
    templates.push(templ("multiset-theory", MULTISET_ARRAY_THEORY));
    templates.push(templ("table-theory", TABLE_ARRAY_THEORY));

    let mut context = Context::new();
    context.insert("options", options);

    let mono_info = mono_analysis::get_info(env);
    // Add vector instances implicitly used by the prelude.
    let implicit_vec_inst = vec![TypeInfo::new(
        env,
        options,
        &Type::Primitive(PrimitiveType::U8),
        false,
    )];
    // Used for generating functions for bv types in prelude
    let mut sh_instances = vec![8, 16, 32, 64, 128, 256];
    let mut bv_instances = bv_helper();
    // Skip bv for cvc5
    if options.use_cvc5 {
        sh_instances = vec![];
        bv_instances = vec![];
    }
    context.insert("sh_instances", &sh_instances);
    context.insert("bv_instances", &bv_instances);
    let mut vec_instances = mono_info
        .vec_inst
        .iter()
        .map(|ty| TypeInfo::new(env, options, ty, false))
        .chain(implicit_vec_inst.into_iter())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect_vec();
    let mut table_instances = mono_info
        .table_inst
        .iter()
        .map(|(qid, ty_args)| MapImpl::new(env, options, *qid, ty_args, false))
        .collect_vec();
    // If not using cvc5, generate vector functions for bv types
    if !options.use_cvc5 {
        let mut bv_vec_instances = mono_info
            .vec_inst
            .iter()
            .map(|ty| TypeInfo::new(env, options, ty, true))
            .filter(|ty_info| !vec_instances.contains(ty_info))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect_vec();
        let mut bv_table_instances = mono_info
            .table_inst
            .iter()
            .map(|(qid, ty_args)| {
                let v_ty = ty_args.iter().map(|(_, vty)| vty).collect_vec();
                let bv_flag = v_ty.iter().all(|ty| ty.skip_reference().is_number());
                MapImpl::new(env, options, *qid, ty_args, bv_flag)
            })
            .filter(|map_impl| !table_instances.contains(map_impl))
            .collect_vec();
        vec_instances.append(&mut bv_vec_instances);
        table_instances.append(&mut bv_table_instances);
    }
    context.insert("vec_instances", &vec_instances);
    context.insert("table_instances", &table_instances);
    let table_key_instances = mono_info
        .table_inst
        .iter()
        .flat_map(|(_, ty_args)| ty_args.iter().map(|(kty, _)| kty))
        .unique()
        .map(|ty| TypeInfo::new(env, options, ty, false))
        .collect_vec();
    context.insert("table_key_instances", &table_key_instances);
    let filter_native = |module: &str| {
        mono_info
            .native_inst
            .iter()
            .filter(|(id, _)| env.get_module(**id).get_full_name_str() == module)
            .flat_map(|(_, insts)| {
                insts.iter().map(|inst| {
                    inst.iter()
                        .map(|i| TypeInfo::new(env, options, i, false))
                        .collect::<Vec<_>>()
                })
            })
            .sorted()
            .collect_vec()
    };
    // make sure that all natives have only one type instantiations
    // because of this assertion, this function returns a `Vec<TypeInfo>`
    let filter_native_ensure_one_inst = |module: &str| {
        filter_native(module)
            .into_iter()
            .map(|mut insts| {
                assert_eq!(insts.len(), 1);
                insts.pop().unwrap()
            })
            .sorted()
            .collect_vec()
    };
    // make sure that all natives have exactly the same number of type instantiations,
    // this function returns a `Vec<Vec<TypeInfo>>`
    let filter_native_check_consistency = |module: &str| {
        let filtered = filter_native(module);
        let size = match filtered.first() {
            None => 0,
            Some(insts) => insts.len(),
        };
        assert!(filtered.iter().all(|insts| insts.len() == size));
        filtered
    };

    let bcs_instances = filter_native_ensure_one_inst(BCS_MODULE);
    context.insert("bcs_instances", &bcs_instances);
    let event_instances = filter_native_ensure_one_inst(EVENT_MODULE);
    context.insert("event_instances", &event_instances);

    // TODO: we have defined {{std}} for adaptable resolution of stdlib addresses but
    //   not used it yet in the templates.
    let std_addr = format!("${}", env.get_stdlib_address());
    let ext_addr = format!("${}", env.get_extlib_address());
    context.insert("std", &std_addr);
    context.insert("Ext", &ext_addr);

    // If a custom Boogie template is provided, add it as part of the templates and
    // add all type instances that use generic functions in the provided modules to the context.
    if let Some(custom_native_options) = options.custom_natives.clone() {
        templates.push(templ(
            "custom-natives",
            &custom_native_options.template_bytes,
        ));
        for (module_name, instance_name, expect_single_type_inst) in
            custom_native_options.module_instance_names
        {
            if expect_single_type_inst {
                context.insert(instance_name, &filter_native_ensure_one_inst(&module_name));
            } else {
                context.insert(
                    instance_name,
                    &filter_native_check_consistency(&module_name),
                );
            }
        }
    }

    let mut tera = Tera::default();
    tera.add_raw_templates(templates)?;

    let expanded_content = tera.render("prelude", &context)?;
    emitln!(writer, &expanded_content);
    Ok(())
}

impl TypeInfo {
    fn new(env: &GlobalEnv, options: &BoogieOptions, ty: &Type, bv_flag: bool) -> Self {
        let name_fun = if bv_flag { boogie_bv_type } else { boogie_type };
        Self {
            name: name_fun(env, ty),
            suffix: boogie_type_suffix_bv(env, ty, bv_flag),
            has_native_equality: has_native_equality(env, options, ty),
        }
    }
}

impl MapImpl {
    fn new(
        env: &GlobalEnv,
        options: &BoogieOptions,
        struct_qid: QualifiedId<StructId>,
        ty_args: &BTreeSet<(Type, Type)>,
        bv_flag: bool,
    ) -> Self {
        let insts = ty_args
            .iter()
            .map(|(kty, vty)| {
                (
                    TypeInfo::new(env, options, kty, false),
                    TypeInfo::new(env, options, vty, bv_flag),
                )
            })
            .collect();
        let struct_env = env.get_struct(struct_qid);
        let struct_name = format!(
            "${}_{}",
            boogie_module_name(&struct_env.module_env),
            struct_env.get_name().display(struct_env.symbol_pool()),
        );

        let decl = env
            .intrinsics
            .get_decl_for_struct(&struct_qid)
            .expect("intrinsic decl");

        MapImpl {
            struct_name,
            insts,
            fun_new: Self::triple_opt_to_name(decl.get_fun_triple(env, INTRINSIC_FUN_MAP_NEW)),
            fun_destroy_empty: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_DESTROY_EMPTY),
            ),
            fun_len: Self::triple_opt_to_name(decl.get_fun_triple(env, INTRINSIC_FUN_MAP_LEN)),
            fun_is_empty: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_IS_EMPTY),
            ),
            fun_has_key: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_HAS_KEY),
            ),
            fun_add_no_override: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_ADD_NO_OVERRIDE),
            ),
            fun_add_override_if_exists: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_ADD_OVERRIDE_IF_EXISTS),
            ),
            fun_del_must_exist: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_DEL_MUST_EXIST),
            ),
            fun_del_return_key: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_DEL_RETURN_KEY),
            ),
            fun_borrow: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_BORROW),
            ),
            fun_borrow_mut: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_BORROW_MUT),
            ),
            fun_spec_new: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_SPEC_NEW),
            ),
            fun_spec_get: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_SPEC_GET),
            ),
            fun_spec_set: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_SPEC_SET),
            ),
            fun_spec_del: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_SPEC_DEL),
            ),
            fun_spec_len: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_SPEC_LEN),
            ),
            fun_spec_is_empty: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_SPEC_IS_EMPTY),
            ),
            fun_spec_has_key: Self::triple_opt_to_name(
                decl.get_fun_triple(env, INTRINSIC_FUN_MAP_SPEC_HAS_KEY),
            ),
        }
    }

    fn triple_opt_to_name(triple_opt: Option<(BigUint, String, String)>) -> String {
        match triple_opt {
            None => String::new(),
            Some((addr, mod_name, fun_name)) => {
                format!("${}_{}_{}", addr.to_str_radix(16), mod_name, fun_name)
            }
        }
    }
}
