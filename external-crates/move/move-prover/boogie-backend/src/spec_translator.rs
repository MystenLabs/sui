// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module translates specification conditions to Boogie code.

use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    rc::Rc,
};

use itertools::Itertools;
#[allow(unused_imports)]
use log::{debug, info, warn};

use move_model::{
    ast::{
        Exp, ExpData, LocalVarDecl, MemoryLabel, Operation, QuantKind, SpecFunDecl, SpecVarDecl,
        TempIndex, Value,
    },
    code_writer::CodeWriter,
    emit, emitln,
    model::{
        FieldId, GlobalEnv, Loc, ModuleEnv, ModuleId, NodeId, QualifiedInstId, SpecFunId,
        SpecVarId, StructId,
    },
    pragmas::INTRINSIC_TYPE_MAP,
    symbol::Symbol,
    ty::{PrimitiveType, Type},
    well_known::{TYPE_INFO_SPEC, TYPE_NAME_GET_SPEC, TYPE_NAME_SPEC, TYPE_SPEC_IS_STRUCT},
};
use move_stackless_bytecode::{
    mono_analysis::MonoInfo,
    number_operation::{GlobalNumberOperationState, NumOperation::Bitwise},
};

use crate::{
    boogie_helpers::{
        boogie_address_blob, boogie_bv_type, boogie_byte_blob, boogie_choice_fun_name,
        boogie_declare_global, boogie_field_sel, boogie_inst_suffix, boogie_modifies_memory_name,
        boogie_num_type_base, boogie_reflection_type_info, boogie_reflection_type_is_struct,
        boogie_reflection_type_name, boogie_resource_memory_name, boogie_spec_fun_name,
        boogie_spec_var_name, boogie_struct_name, boogie_type, boogie_type_suffix,
        boogie_type_suffix_bv, boogie_value_blob, boogie_well_formed_expr,
        boogie_well_formed_expr_bv,
    },
    options::BoogieOptions,
};

#[derive(Clone)]
pub struct SpecTranslator<'env> {
    /// The global environment.
    env: &'env GlobalEnv,
    /// Options passed into the translator.
    options: &'env BoogieOptions,
    /// The code writer.
    writer: &'env CodeWriter,
    /// If we are translating in the context of a type instantiation, the type arguments.
    type_inst: Vec<Type>,
    /// Counter for creating new variables.
    fresh_var_count: RefCell<usize>,
    /// Information about lifted choice expressions. Each choice expression in the
    /// original program is uniquely identified by the choice expression AST (verbatim),
    /// which includes the node id of the expression.
    ///
    /// This allows us to capture duplication of expressions and map them to the same uninterpreted
    /// choice function. If an expression is duplicated and then later specialized by a type
    /// instantiation, it will have a different node id, but again the same instantiations
    /// map to the same node id, which is the desired semantics.
    lifted_choice_infos: Rc<RefCell<HashMap<(ExpData, Vec<Type>), LiftedChoiceInfo>>>,
}

/// A struct which contains information about a lifted choice expression (like `some x:int: p(x)`).
/// Those expressions are replaced by a call to an axiomatized function which is generated from
/// this info at the end of translation.
#[derive(Clone)]
struct LiftedChoiceInfo {
    id: usize,
    node_id: NodeId,
    kind: QuantKind,
    free_vars: Vec<(Symbol, Type)>,
    used_temps: Vec<(TempIndex, Type)>,
    used_memory: Vec<(QualifiedInstId<StructId>, Option<MemoryLabel>)>,
    var: Symbol,
    range: Exp,
    condition: Exp,
}

impl<'env> SpecTranslator<'env> {
    /// Creates a translator.
    pub fn new(
        writer: &'env CodeWriter,
        env: &'env GlobalEnv,
        options: &'env BoogieOptions,
    ) -> Self {
        Self {
            env,
            options,
            writer,
            type_inst: vec![],
            fresh_var_count: Default::default(),
            lifted_choice_infos: Default::default(),
        }
    }

    /// Emits a translation error.
    pub fn error(&self, loc: &Loc, msg: &str) {
        self.env.error(loc, &format!("[boogie translator] {}", msg));
    }

    /// Sets the location of the code writer from node id.
    fn set_writer_location(&self, node_id: NodeId) {
        self.writer.set_location(&self.env.get_node_loc(node_id));
    }

    /// Generates a fresh variable name.
    fn fresh_var_name(&self, prefix: &str) -> String {
        let mut fvc_ref = self.fresh_var_count.borrow_mut();
        let name_str = format!("${}_{}", prefix, *fvc_ref);
        *fvc_ref = usize::saturating_add(*fvc_ref, 1);
        name_str
    }

    /// Translates a sequence of items separated by `sep`.
    fn translate_seq<T, F>(&self, items: impl Iterator<Item = T>, sep: &str, f: F)
    where
        F: Fn(T),
    {
        let mut first = true;
        for item in items {
            if first {
                first = false;
            } else {
                emit!(self.writer, sep);
            }
            f(item);
        }
    }
}

// Axioms
// ======

impl<'env> SpecTranslator<'env> {
    pub fn translate_axioms(&self, env: &GlobalEnv, mono_info: &MonoInfo) {
        let type_display_ctx = env.get_type_display_ctx();
        for (axiom, type_insts) in &mono_info.axioms {
            for type_inst in type_insts {
                self.writer.set_location(&axiom.loc);
                emit!(self.writer, "// axiom {}", axiom.loc.display(env));
                if !type_inst.is_empty() {
                    emitln!(
                        self.writer,
                        ", instance <{}>",
                        type_inst
                            .iter()
                            .map(|t| t.display(&type_display_ctx).to_string())
                            .join(", ")
                    );
                } else {
                    emitln!(self.writer);
                }
                emit!(self.writer, "axiom ");
                self.translate(&axiom.exp, type_inst);
                emitln!(self.writer, ";\n");
            }
        }
    }
}

// Specification Variables
// =======================

impl<'env> SpecTranslator<'env> {
    pub fn translate_spec_vars(&self, module_env: &ModuleEnv<'_>, mono_info: &MonoInfo) {
        let empty = &BTreeSet::new();
        let mut translated = BTreeSet::new();
        for (id, var) in module_env.get_spec_vars() {
            for type_inst in mono_info
                .spec_vars
                .get(&module_env.get_id().qualified(*id))
                .unwrap_or(empty)
                .iter()
                .cloned()
            {
                let name = boogie_spec_var_name(
                    module_env,
                    module_env.get_spec_var(*id).name,
                    &type_inst,
                    &None,
                );
                if !translated.insert(name) {
                    continue;
                }
                if type_inst.is_empty() {
                    self.translate_spec_var(module_env, *id, var);
                } else {
                    SpecTranslator {
                        type_inst,
                        ..self.clone()
                    }
                    .translate_spec_var(module_env, *id, var);
                }
            }
        }
    }

    pub fn translate_spec_var(
        &self,
        module_env: &ModuleEnv<'_>,
        _id: SpecVarId,
        var: &SpecVarDecl,
    ) {
        emitln!(self.writer, "// spec var {}", var.loc.display(self.env));
        let boogie_name = boogie_spec_var_name(module_env, var.name, &self.type_inst, &None);
        emitln!(
            self.writer,
            &boogie_declare_global(self.env, &boogie_name, &self.inst(&var.type_))
        );
    }
}

// Specification Functions
// =======================

impl<'env> SpecTranslator<'env> {
    pub fn translate_spec_funs(&self, module_env: &ModuleEnv<'_>, mono_info: &MonoInfo) {
        let empty = &BTreeSet::new();
        let mut translated = BTreeSet::new();
        for (id, fun) in module_env.get_spec_funs() {
            for type_inst in mono_info
                .spec_funs
                .get(&module_env.get_id().qualified(*id))
                .unwrap_or(empty)
                .iter()
                .cloned()
            {
                let name = boogie_spec_fun_name(module_env, *id, &type_inst, false);
                if !translated.insert(name) {
                    continue;
                }
                if type_inst.is_empty() {
                    self.translate_spec_fun(module_env, *id, fun);
                } else {
                    let new_spec_trans = SpecTranslator {
                        type_inst,
                        ..self.clone()
                    };
                    new_spec_trans.translate_spec_fun(module_env, *id, fun);
                }
            }
        }
    }

    fn translate_spec_fun(&self, module_env: &ModuleEnv, id: SpecFunId, fun: &SpecFunDecl) {
        if fun.body.is_none() && !fun.uninterpreted {
            // This function is native and expected to be found in the prelude.
            return;
        }
        if fun.is_move_fun && fun.is_native {
            // This function is a native Move function and its spec version is
            // expected to be found in the prelude.
            return;
        }
        if fun.is_move_fun && !module_env.spec_fun_is_used(id) {
            // This function is a pure Move function but is never used,
            // so we don't need to translate it.
            return;
        }
        if let Type::Tuple(..) | Type::Fun(..) = fun.result_type {
            self.error(&fun.loc, "function or tuple result type not yet supported");
            return;
        }
        let recursive = self
            .env
            .is_spec_fun_recursive(module_env.get_id().qualified(id));
        emitln!(
            self.writer,
            "// {}spec fun {}",
            if recursive { "recursive " } else { "" },
            fun.loc.display(self.env)
        );
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let bv_flag_result = if global_state
            .spec_fun_operation_map
            .contains_key(&(module_env.get_id(), id))
        {
            let ret_oper_map = &global_state
                .spec_fun_operation_map
                .get(&(module_env.get_id(), id))
                .unwrap()
                .1;
            ret_oper_map[0] == Bitwise
        } else {
            false
        };
        let ty_str_fn = |bv_flag: bool| {
            if bv_flag {
                boogie_bv_type
            } else {
                boogie_type
            }
        };
        let result_type = ty_str_fn(bv_flag_result)(self.env, &self.inst(&fun.result_type));
        // it is possible that the spec fun may refer to the same memory after monomorphization,
        // (e.g., one via concrete type and the other via type parameter being instantiated).
        // In this case, we mark the other parameter as unused
        let mut mem_inst_seen = BTreeSet::new();
        let mem_params = fun.used_memory.iter().map(|memory| {
            let memory = memory.to_owned().instantiate(&self.type_inst);
            let struct_env = &self.env.get_struct_qid(memory.to_qualified_id());
            let param_repr = format!(
                "{}: $Memory {}",
                boogie_resource_memory_name(self.env, &memory, &None),
                boogie_struct_name(struct_env, &memory.inst)
            );
            if mem_inst_seen.insert(memory) {
                param_repr
            } else {
                format!("__unused_{}", param_repr)
            }
        });
        let params = fun.params.iter().enumerate().map(|(i, (name, ty))| {
            let bv_flag = if global_state
                .spec_fun_operation_map
                .contains_key(&(module_env.get_id(), id))
            {
                global_state
                    .spec_fun_operation_map
                    .get(&(module_env.get_id(), id))
                    .unwrap()
                    .0[i]
                    == Bitwise
            } else {
                false
            };
            format!(
                "{}: {}",
                name.display(module_env.symbol_pool()),
                ty_str_fn(bv_flag)(self.env, &self.inst(ty))
            )
        });
        self.writer.set_location(&fun.loc);
        let boogie_name = boogie_spec_fun_name(module_env, id, &self.type_inst, bv_flag_result);
        let param_list = mem_params.chain(params).join(", ");
        let attrs = if fun.uninterpreted || recursive {
            ""
        } else {
            "{:inline}"
        };
        emit!(
            self.writer,
            "function {} {}({}): {}",
            attrs,
            boogie_name,
            param_list,
            result_type
        );
        if fun.uninterpreted {
            // Uninterpreted function has no body.
            emitln!(self.writer, ";");
            // Emit axiom about return type. Notice we don't need to process spec_var or memory
            // parameters because an interpreted functions does not have those.
            let call = format!(
                "{}({})",
                boogie_name,
                fun.params
                    .iter()
                    .map(|(n, _)| { format!("{}", n.display(module_env.symbol_pool())) })
                    .join(", ")
            );
            let type_check =
                boogie_well_formed_expr(self.env, "$$res", &self.inst(&fun.result_type));
            if !type_check.is_empty() {
                if !param_list.is_empty() {
                    emitln!(
                        self.writer,
                        "axiom (forall {} ::\n(var $$res := {};\n{}));",
                        param_list,
                        call,
                        type_check
                    );
                } else {
                    emitln!(
                        self.writer,
                        "axiom (var $$res := {};\n{});",
                        call,
                        type_check
                    );
                }
            }
        } else {
            emitln!(self.writer, " {");
            self.writer.indent();
            self.translate_exp(fun.body.as_ref().unwrap());
            emitln!(self.writer);
            self.writer.unindent();
            emitln!(self.writer, "}");
        }

        emitln!(self.writer);
    }
}

// Emit any finalization items
// ============================

impl<'env> SpecTranslator<'env> {
    pub(crate) fn finalize(&self) {
        self.translate_choice_functions();
    }

    /// Translate lifted functions for choice expressions.
    fn translate_choice_functions(&self) {
        let env = self.env;
        let infos_ref = self.lifted_choice_infos.borrow();
        // need the sorting here because `lifted_choice_infos` is a hashmap while we want
        // deterministic ordering of the output. Sorting uses the `.id` field, which represents the
        // insertion order.
        let infos_sorted_with_keys = infos_ref.iter().sorted_by(|v1, v2| v1.1.id.cmp(&v2.1.id));
        assert!(self.type_inst.is_empty());
        for (key, info) in infos_sorted_with_keys {
            let fun_name = boogie_choice_fun_name(info.id);
            let result_ty = &env.get_node_type(info.node_id);
            let exp_loc = env.get_node_loc(info.node_id);
            let var_name = info.var.display(env.symbol_pool()).to_string();
            self.writer.set_location(&exp_loc);

            let new_spec_trans = SpecTranslator {
                type_inst: key.1.clone(),
                ..self.clone()
            };

            // Pairs of context parameter names and boogie types
            let param_decls = info
                .free_vars
                .iter()
                .map(|(s, ty)| {
                    (
                        s.display(env.symbol_pool()).to_string(),
                        boogie_type(env, ty.skip_reference()),
                    )
                })
                .chain(
                    info.used_temps
                        .iter()
                        .map(|(t, ty)| (format!("$t{}", t), boogie_type(env, ty.skip_reference()))),
                )
                .chain(info.used_memory.iter().map(|(m, l)| {
                    let struct_env = &env.get_struct(m.to_qualified_id());
                    (
                        boogie_resource_memory_name(env, m, l),
                        format!("$Memory {}", boogie_struct_name(struct_env, &m.inst)),
                    )
                }))
                .collect_vec();
            // Pair of choice variable name and type.
            let var_decl = (var_name, boogie_type(env, result_ty));

            // Helper functions
            let mk_decl = |(n, t): &(String, String)| format!("{}: {}", n, t);
            let mk_arg = |(n, _): &(String, String)| n.to_owned();
            let emit_valid = |n: &str, ty: &Type| {
                let suffix = boogie_type_suffix(env, ty.skip_reference());
                emit!(new_spec_trans.writer, "$IsValid'{}'({})", suffix, n);
            };
            let mk_temp = |t: TempIndex| format!("$t{}", t);

            emitln!(
                new_spec_trans.writer,
                "// choice expression {}",
                exp_loc.display(new_spec_trans.env)
            );

            // Emit predicate function characterizing the choice.
            emitln!(
                new_spec_trans.writer,
                "function {{:inline}} {}_pred({}): bool {{",
                fun_name,
                vec![&var_decl]
                    .into_iter()
                    .chain(param_decls.iter())
                    .map(mk_decl)
                    .join(", ")
            );
            new_spec_trans.writer.indent();
            emit_valid(&var_decl.0, result_ty);
            match env.get_node_type(info.range.node_id()) {
                Type::Vector(..) => {
                    emit!(new_spec_trans.writer, " && InRangeVec(");
                    new_spec_trans.translate_exp(&info.range);
                    emit!(new_spec_trans.writer, ", {})", &var_decl.0);
                }
                Type::Primitive(PrimitiveType::Range) => {
                    emit!(new_spec_trans.writer, " && $InRange(");
                    new_spec_trans.translate_exp(&info.range);
                    emit!(new_spec_trans.writer, ", {})", &var_decl.0);
                }
                Type::Primitive(_)
                | Type::Tuple(_)
                | Type::Struct(_, _, _)
                | Type::TypeParameter(_)
                | Type::Reference(_, _)
                | Type::Fun(_, _)
                | Type::TypeDomain(_)
                | Type::ResourceDomain(_, _, _)
                | Type::Error
                | Type::Var(_) => {}
            }
            emitln!(new_spec_trans.writer, " &&");
            new_spec_trans.translate_exp(&info.condition);
            new_spec_trans.writer.unindent();
            emitln!(new_spec_trans.writer, "\n}");
            // Create call to predicate
            let predicate = format!(
                "{}_pred({})",
                fun_name,
                vec![&var_decl]
                    .into_iter()
                    .chain(param_decls.iter())
                    .map(mk_arg)
                    .join(", ")
            );

            // Emit choice function
            emitln!(
                new_spec_trans.writer,
                "function {}({}): {};",
                fun_name,
                param_decls.iter().map(mk_decl).join(", "),
                boogie_type(env, result_ty)
            );
            // Create call to choice function
            let choice = format!(
                "{}({})",
                fun_name,
                param_decls.iter().map(mk_arg).join(", ")
            );

            // Emit choice axiom
            if !param_decls.is_empty() {
                emit!(
                    new_spec_trans.writer,
                    "axiom (forall {}:: ",
                    param_decls.iter().map(mk_decl).join(", ")
                );
                if !info.free_vars.is_empty() || !info.used_temps.is_empty() {
                    // TODO: IsValid for memory?
                    let mut sep = "";
                    for (s, ty) in &info.free_vars {
                        emit!(new_spec_trans.writer, sep);
                        emit_valid(env.symbol_pool().string(*s).as_ref(), ty);
                        sep = " && ";
                    }
                    for (t, ty) in &info.used_temps {
                        emit!(new_spec_trans.writer, sep);
                        emit_valid(&mk_temp(*t), ty);
                        sep = " && ";
                    }
                    emitln!(new_spec_trans.writer, " ==>");
                }
            } else {
                emitln!(new_spec_trans.writer, "axiom");
            }
            new_spec_trans.writer.indent();
            emitln!(
                new_spec_trans.writer,
                "(exists {}:: {}) ==> ",
                mk_decl(&var_decl),
                predicate
            );
            emitln!(
                new_spec_trans.writer,
                "(var {} := {}; {}",
                &var_decl.0,
                choice,
                predicate
            );

            // Emit min constraint
            if info.kind == QuantKind::ChooseMin {
                // Check whether we support min on the range type.
                if !result_ty.is_number() && !result_ty.is_signer_or_address() {
                    env.error(
                        &env.get_node_loc(info.node_id),
                        "The min choice can only be applied to numbers, addresses, or signers",
                    )
                }
                // Add the condition that there does not exist a smaller satisfying value.
                emit!(new_spec_trans.writer, " && (var $$c := {}; ", &var_decl.0);
                emit!(
                    new_spec_trans.writer,
                    "(forall {}:: {} < $$c ==> !{}))",
                    mk_decl(&var_decl),
                    &var_decl.0,
                    predicate
                );
            }
            new_spec_trans.writer.unindent();
            if !param_decls.is_empty() {
                emit!(new_spec_trans.writer, ")");
            }
            emitln!(new_spec_trans.writer, ");\n");
        }
    }
}

// Expressions
// ===========

impl<'env> SpecTranslator<'env> {
    pub(crate) fn translate(&self, exp: &Exp, type_inst: &[Type]) {
        *self.fresh_var_count.borrow_mut() = 0;
        if type_inst.is_empty() {
            self.translate_exp(exp)
        } else {
            // Use a clone with the given type instantiation.
            let mut trans = self.clone();
            trans.type_inst = type_inst.to_owned();
            trans.translate_exp(exp)
        }
    }

    fn inst(&self, ty: &Type) -> Type {
        ty.instantiate(&self.type_inst)
    }

    fn inst_slice(&self, tys: &[Type]) -> Vec<Type> {
        Type::instantiate_slice(tys, &self.type_inst)
    }

    fn get_node_type(&self, id: NodeId) -> Type {
        self.inst(&self.env.get_node_type(id))
    }

    fn get_node_instantiation(&self, id: NodeId) -> Vec<Type> {
        self.inst_slice(&self.env.get_node_instantiation(id))
    }

    fn translate_exp(&self, exp: &Exp) {
        match exp.as_ref() {
            ExpData::Value(node_id, val) => {
                self.set_writer_location(*node_id);
                self.translate_value(*node_id, val);
            }
            ExpData::LocalVar(node_id, name) => {
                self.set_writer_location(*node_id);
                self.translate_local_var(*node_id, *name);
            }
            ExpData::Temporary(node_id, idx) => {
                self.set_writer_location(*node_id);
                self.translate_temporary(*node_id, *idx);
            }
            ExpData::Call(node_id, oper, args) => {
                self.set_writer_location(*node_id);
                self.translate_call(*node_id, oper, args);
            }
            ExpData::Invoke(node_id, ..) => {
                self.error(&self.env.get_node_loc(*node_id), "Invoke not yet supported")
            }
            ExpData::Lambda(node_id, ..) => self.error(
                &self.env.get_node_loc(*node_id),
                "`|x|e` (lambda) currently only supported as argument for `all` or `any`",
            ),
            ExpData::Quant(node_id, kind, ranges, _, _, exp) if kind.is_choice() => {
                // The parser ensures that len(ranges) = 1 and triggers and condition are
                // not present.
                self.set_writer_location(*node_id);
                self.translate_choice(*node_id, *kind, &ranges[0], exp)
            }
            ExpData::Quant(node_id, kind, ranges, triggers, condition, exp) => {
                self.set_writer_location(*node_id);
                self.translate_quant(*node_id, *kind, ranges, triggers, condition, exp)
            }
            ExpData::Block(node_id, vars, scope) => {
                self.set_writer_location(*node_id);
                self.translate_block(vars, scope)
            }
            ExpData::IfElse(node_id, cond, on_true, on_false) => {
                self.set_writer_location(*node_id);
                // The whole ITE is one expression so we wrap it with a parenthesis
                emit!(self.writer, "(");
                emit!(self.writer, "if ");
                self.translate_exp_parenthesised(cond);
                emit!(self.writer, " then ");
                self.translate_exp_parenthesised(on_true);
                emit!(self.writer, " else ");
                self.translate_exp_parenthesised(on_false);
                emit!(self.writer, ")");
            }
            ExpData::Invalid(_) => panic!("unexpected error expression"),
        }
    }

    fn translate_exp_parenthesised(&self, exp: &Exp) {
        emit!(self.writer, "(");
        self.translate_exp(exp);
        emit!(self.writer, ")");
    }

    fn translate_value(&self, node_id: NodeId, val: &Value) {
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let num_oper = global_state.get_node_num_oper(node_id);
        let mut suffix = "".to_string();
        let bv_flag = num_oper == Bitwise;
        if bv_flag {
            suffix = boogie_bv_type(self.env, self.env.get_node_type(node_id).skip_reference());
        }
        match val {
            Value::Address(addr) => emit!(self.writer, "{}", addr),
            Value::Number(val) => emit!(self.writer, "{}{}", val, suffix),
            Value::Bool(val) => emit!(self.writer, "{}", val),
            Value::ByteArray(val) => {
                emit!(self.writer, &boogie_byte_blob(self.options, val, bv_flag))
            }
            Value::AddressArray(val) => emit!(self.writer, &boogie_address_blob(self.options, val)),
            Value::Vector(val) => emit!(self.writer, &boogie_value_blob(self.options, val)),
        }
    }

    fn translate_local_var(&self, _node_id: NodeId, name: Symbol) {
        emit!(self.writer, "{}", name.display(self.env.symbol_pool()));
    }

    fn translate_temporary(&self, node_id: NodeId, idx: TempIndex) {
        let ty = self.get_node_type(node_id);
        let mut_ref = ty.is_mutable_reference();
        if mut_ref {
            emit!(self.writer, "$Dereference(");
        }
        emit!(self.writer, "$t{}", idx);
        if mut_ref {
            emit!(self.writer, ")")
        }
    }

    fn translate_block(&self, vars: &[LocalVarDecl], exp: &Exp) {
        if vars.is_empty() {
            return self.translate_exp(exp);
        }
        let mut bracket_num = 0;
        for var in vars {
            let name_str = self.env.symbol_pool().string(var.name);
            emit!(self.writer, "(var {} := ", name_str);
            self.translate_exp(var.binding.as_ref().expect("binding"));
            emit!(self.writer, "; ");
            bracket_num += 1;
        }
        self.translate_exp(exp);
        for _n in 0..bracket_num {
            emit!(self.writer, ")");
        }
    }

    fn translate_call(&self, node_id: NodeId, oper: &Operation, args: &[Exp]) {
        let loc = self.env.get_node_loc(node_id);
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        match oper {
            // Operators we introduced in the top level public entry `SpecTranslator::translate`,
            // mapping between Boogies single value domain and our typed world.
            Operation::BoxValue | Operation::UnboxValue => panic!("unexpected box/unbox"),

            // Internal operators for event stores.
            Operation::EmptyEventStore => emit!(self.writer, "$EmptyEventStore"),
            Operation::ExtendEventStore => self.translate_extend_event_store(args),
            Operation::EventStoreIncludes => self.translate_event_store_includes(args),
            Operation::EventStoreIncludedIn => self.translate_event_store_included_in(args),

            // Regular expressions
            Operation::Function(module_id, fun_id, memory_labels) => {
                self.translate_spec_fun_call(node_id, *module_id, *fun_id, args, memory_labels)
            }
            Operation::Pack(mid, sid) => self.translate_pack(node_id, *mid, *sid, args),
            Operation::Tuple => self.error(&loc, "Tuple not yet supported"),
            Operation::Select(module_id, struct_id, field_id) => {
                self.translate_select(node_id, *module_id, *struct_id, *field_id, args)
            }
            Operation::UpdateField(module_id, struct_id, field_id) => {
                self.translate_update_field(node_id, *module_id, *struct_id, *field_id, args)
            }
            Operation::Result(pos) => {
                emit!(self.writer, "$ret{}", pos);
            }
            Operation::Index => self.translate_primitive_call("ReadVec", args),
            Operation::Slice => self.translate_primitive_call("$SliceVecByRange", args),
            Operation::Range => self.translate_primitive_call("$Range", args),

            // Binary operators
            Operation::Add => self.translate_op("+", "Add", args),
            Operation::Sub => self.translate_op("-", "Sub", args),
            Operation::Mul => self.translate_op("*", "Mul", args),
            Operation::Mod => self.translate_op("mod", "Mod", args),
            Operation::Div => self.translate_op("div", "Div", args),
            Operation::BitOr => self.translate_bit_op("$Or", args),
            Operation::BitAnd => self.translate_bit_op("$And", args),
            Operation::Xor => self.translate_bit_op("$Xor", args),
            Operation::Shl => self.translate_primitive_call_shl("$shl", args),
            Operation::Shr => self.translate_primitive_call_shr("$shr", args),
            Operation::Implies => self.translate_logical_op("==>", args),
            Operation::Iff => self.translate_logical_op("<==>", args),
            Operation::And => self.translate_logical_op("&&", args),
            Operation::Or => self.translate_logical_op("||", args),
            Operation::Lt => self.translate_op("<", "Lt", args),
            Operation::Le => self.translate_op("<=", "Le", args),
            Operation::Gt => self.translate_op(">", "Gt", args),
            Operation::Ge => self.translate_op(">=", "Ge", args),
            Operation::Identical => self.translate_identical(args),
            Operation::Eq => self.translate_eq_neq("$IsEqual", args),
            Operation::Neq => self.translate_eq_neq("!$IsEqual", args),

            // Unary operators
            Operation::Not => self.translate_logical_unary_op("!", args),
            Operation::Cast => self.translate_cast(node_id, args),
            Operation::Int2Bv => {
                let exp_arith_flag = global_state.get_node_num_oper(args[0].node_id()) != Bitwise;
                if exp_arith_flag {
                    let arg_node_type = self.env.get_node_type(args[0].node_id());
                    let literal = boogie_num_type_base(&arg_node_type);
                    emit!(self.writer, "$int2bv.{}(", literal);
                }
                self.translate_exp(&args[0]);
                if exp_arith_flag {
                    emit!(self.writer, ")");
                }
            }
            Operation::Bv2Int => {
                let exp_bv_flag = global_state.get_node_num_oper(args[0].node_id()) == Bitwise;
                if exp_bv_flag {
                    let arg_node_type = self.env.get_node_type(args[0].node_id());
                    let literal = boogie_num_type_base(&arg_node_type);
                    emit!(self.writer, "$bv2int.{}(", literal);
                }
                self.translate_exp(&args[0]);
                if exp_bv_flag {
                    emit!(self.writer, ")");
                }
            }
            // Builtin functions
            Operation::Global(memory_label) => {
                self.translate_resource_access(node_id, args, memory_label)
            }
            Operation::Exists(memory_label) => {
                self.translate_resource_exists(node_id, args, memory_label)
            }
            Operation::CanModify => self.translate_can_modify(node_id, args),
            Operation::Len => self.translate_primitive_call("LenVec", args),
            Operation::TypeValue => self.translate_type_value(node_id),
            Operation::TypeDomain | Operation::ResourceDomain => self.error(
                &loc,
                "domain functions can only be used as the range of a quantifier",
            ),
            Operation::UpdateVec => self.translate_primitive_call("UpdateVec", args),
            Operation::ConcatVec => self.translate_primitive_call("ConcatVec", args),
            Operation::EmptyVec => self.translate_primitive_inst_call(node_id, "$EmptyVec", args),
            Operation::SingleVec => self.translate_primitive_call("MakeVec1", args),
            Operation::IndexOfVec => {
                self.translate_primitive_inst_call(node_id, "$IndexOfVec", args)
            }
            Operation::ContainsVec => {
                self.translate_primitive_inst_call(node_id, "$ContainsVec", args)
            }
            Operation::RangeVec => self.translate_primitive_inst_call(node_id, "$RangeVec", args),
            Operation::InRangeVec => self.translate_primitive_call("InRangeVec", args),
            Operation::InRangeRange => self.translate_primitive_call("$InRange", args),
            Operation::MaxU8 => emit!(self.writer, "$MAX_U8"),
            Operation::MaxU16 => emit!(self.writer, "$MAX_U16"),
            Operation::MaxU32 => emit!(self.writer, "$MAX_U32"),
            Operation::MaxU64 => emit!(self.writer, "$MAX_U64"),
            Operation::MaxU128 => emit!(self.writer, "$MAX_U128"),
            Operation::MaxU256 => emit!(self.writer, "$MAX_U256"),
            Operation::WellFormed => self.translate_well_formed(&args[0]),
            Operation::AbortCode => emit!(self.writer, "$abort_code"),
            Operation::AbortFlag => emit!(self.writer, "$abort_flag"),
            Operation::NoOp => { /* do nothing. */ }
            Operation::Trace(_) => {
                // An unreduced trace means it has been used in a spec fun or let.
                // Create an error about this.
                self.env.error(
                    &loc,
                    "currently `TRACE(..)` cannot be used in spec functions or in lets",
                )
            }
            Operation::Old => panic!("operation unexpected: {:?}", oper),
        }
    }

    fn translate_event_store_includes(&self, args: &[Exp]) {
        emit!(
            self.writer,
            "(var actual := $EventStore__subtract($es, old($es)); "
        );
        emit!(self.writer, "(var expected := ");
        self.translate_exp(&args[0]);
        emit!(self.writer, "; $EventStore__is_subset(expected, actual)))");
    }

    fn translate_event_store_included_in(&self, args: &[Exp]) {
        emit!(
            self.writer,
            "(var actual := $EventStore__subtract($es, old($es)); "
        );
        emit!(self.writer, "(var expected := ");
        self.translate_exp(&args[0]);
        emit!(self.writer, "; $EventStore__is_subset(actual, expected)))");
    }

    fn translate_extend_event_store(&self, args: &[Exp]) {
        let suffix = boogie_type_suffix(self.env, &self.get_node_type(args[1].node_id()));
        let with_cond = args.len() == 4;
        if with_cond {
            emit!(self.writer, "$CondExtendEventStore'{}'(", suffix)
        } else {
            emit!(self.writer, "$ExtendEventStore'{}'(", suffix)
        }
        self.translate_exp(&args[0]); // event store
        emit!(self.writer, ", ");
        // Next expected argument is the handle.
        self.translate_exp(&args[2]);
        emit!(self.writer, ", ");
        // Next comes the event.
        self.translate_exp(&args[1]);
        // Next comes the optional condition
        if with_cond {
            emit!(self.writer, ", ");
            self.translate_exp(&args[3]);
        }
        emit!(self.writer, ")");
    }

    fn translate_pack(&self, node_id: NodeId, mid: ModuleId, sid: StructId, args: &[Exp]) {
        let struct_env = &self.env.get_module(mid).into_struct(sid);
        let inst = &self.get_node_instantiation(node_id);
        emit!(self.writer, "{}(", boogie_struct_name(struct_env, inst));
        let mut sep = "";
        for arg in args {
            emit!(self.writer, sep);
            self.translate_exp(arg);
            sep = ", ";
        }
        emit!(self.writer, ")");
    }

    fn translate_spec_fun_call(
        &self,
        node_id: NodeId,
        module_id: ModuleId,
        fun_id: SpecFunId,
        args: &[Exp],
        memory_labels: &Option<Vec<MemoryLabel>>,
    ) {
        let inst = &self.get_node_instantiation(node_id);
        let module_env = &self.env.get_module(module_id);
        let fun_decl = module_env.get_spec_fun(fun_id);
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");

        // special casing for type reflection
        let mut processed = false;

        // TODO(mengxu): change it to a better address name instead of extlib
        if self.env.get_extlib_address() == *module_env.get_name().addr() {
            let qualified_name = format!(
                "{}::{}",
                module_env.get_name().name().display(self.env.symbol_pool()),
                fun_decl.name.display(self.env.symbol_pool()),
            );
            if qualified_name == TYPE_NAME_SPEC {
                assert_eq!(inst.len(), 1);
                emit!(
                    self.writer,
                    "{}",
                    boogie_reflection_type_name(self.env, &inst[0], false)
                );
                processed = true;
            } else if qualified_name == TYPE_INFO_SPEC {
                assert_eq!(inst.len(), 1);
                // TODO(mengxu): by ignoring the first return value of this function, we are
                // essentially ignoring the condition where this `type_info` call may abort, e.g.,
                // invoking `type_info` on a primitive type like: `type_info<bool>`.
                let (_, info) = boogie_reflection_type_info(self.env, &inst[0]);
                emit!(self.writer, "{}", info);
                processed = true;
            } else if qualified_name == TYPE_SPEC_IS_STRUCT {
                assert_eq!(inst.len(), 1);
                emit!(
                    self.writer,
                    "{}",
                    boogie_reflection_type_is_struct(self.env, &inst[0])
                );
                processed = true;
            }
        }

        if self.env.get_stdlib_address() == *module_env.get_name().addr() {
            let qualified_name = format!(
                "{}::{}",
                module_env.get_name().name().display(self.env.symbol_pool()),
                fun_decl.name.display(self.env.symbol_pool()),
            );
            if qualified_name == TYPE_NAME_GET_SPEC {
                assert_eq!(inst.len(), 1);
                emit!(
                    self.writer,
                    "{}",
                    boogie_reflection_type_name(self.env, &inst[0], true)
                );
                processed = true;
            }
        }

        let is_vector_table_module = module_env.is_std_vector() || module_env.is_table();
        // regular path
        if !processed {
            let bv_flag = if is_vector_table_module && !args.is_empty() {
                global_state.get_node_num_oper(args[0].node_id()) == Bitwise
            } else {
                global_state.get_node_num_oper(node_id) == Bitwise
            };
            let name = boogie_spec_fun_name(module_env, fun_id, inst, bv_flag);
            emit!(self.writer, "{}(", name);
            let mut first = true;
            let mut maybe_comma = || {
                if first {
                    first = false;
                } else {
                    emit!(self.writer, ", ");
                }
            };
            let label_at = |i| memory_labels.as_ref().map(|labels| labels[i]);
            let mut i = 0;
            for memory in &fun_decl.used_memory {
                let memory = &memory.to_owned().instantiate(inst);
                maybe_comma();
                let memory = boogie_resource_memory_name(self.env, memory, &label_at(i));
                emit!(self.writer, &memory);
                i = usize::saturating_add(i, 1);
            }
            for exp in args {
                maybe_comma();
                self.translate_exp(exp);
            }
            emit!(self.writer, ")");
        }
    }

    fn translate_select(
        &self,
        node_id: NodeId,
        module_id: ModuleId,
        struct_id: StructId,
        field_id: FieldId,
        args: &[Exp],
    ) {
        let struct_env = self.env.get_module(module_id).into_struct(struct_id);
        if struct_env.is_native_or_intrinsic() {
            self.env.error(
                &self.env.get_node_loc(node_id),
                "cannot select field of intrinsic struct",
            );
        }
        let struct_type = &self.get_node_type(args[0].node_id());
        let (_, _, inst) = struct_type.skip_reference().require_struct();
        let field_env = struct_env.get_field(field_id);
        emit!(self.writer, "{}(", boogie_field_sel(&field_env, inst));
        self.translate_exp(&args[0]);
        emit!(self.writer, ")");
    }

    fn translate_update_field(
        &self,
        node_id: NodeId,
        module_id: ModuleId,
        struct_id: StructId,
        field_id: FieldId,
        args: &[Exp],
    ) {
        let struct_env = &self.env.get_module(module_id).into_struct(struct_id);
        let field_env = struct_env.get_field(field_id);
        let suffix = boogie_inst_suffix(self.env, &self.get_node_instantiation(node_id));
        emit!(
            self.writer,
            "$Update{}_{}(",
            suffix,
            field_env.get_name().display(self.env.symbol_pool())
        );
        self.translate_exp(&args[0]);
        emit!(self.writer, ", ");
        self.translate_exp(&args[1]);
        emit!(self.writer, ")");
    }

    fn translate_type_value(&self, node_id: NodeId) {
        let loc = &self.env.get_node_loc(node_id);
        self.env
            .error(loc, "type values not supported by this backend");
    }

    fn translate_resource_access(
        &self,
        node_id: NodeId,
        args: &[Exp],
        memory_label: &Option<MemoryLabel>,
    ) {
        let memory = &self.get_memory_inst_from_node(node_id);
        emit!(
            self.writer,
            "$ResourceValue({}, ",
            boogie_resource_memory_name(self.env, memory, memory_label),
        );
        self.translate_exp(&args[0]);
        emit!(self.writer, ")");
    }

    fn get_memory_inst_from_node(&self, node_id: NodeId) -> QualifiedInstId<StructId> {
        let mem_ty = &self.get_node_instantiation(node_id)[0];
        let (mid, sid, inst) = mem_ty.require_struct();
        mid.qualified_inst(sid, inst.to_owned())
    }

    fn translate_resource_exists(
        &self,
        node_id: NodeId,
        args: &[Exp],
        memory_label: &Option<MemoryLabel>,
    ) {
        let memory = &self.get_memory_inst_from_node(node_id);
        emit!(
            self.writer,
            "$ResourceExists({}, ",
            boogie_resource_memory_name(self.env, memory, memory_label),
        );
        self.translate_exp(&args[0]);
        emit!(self.writer, ")");
    }

    fn translate_can_modify(&self, node_id: NodeId, args: &[Exp]) {
        let memory = &self.get_memory_inst_from_node(node_id);
        let resource_name = boogie_modifies_memory_name(self.env, memory);
        emit!(self.writer, "{}[", resource_name);

        let is_signer = self.env.get_node_type(args[0].node_id()).is_signer();
        if is_signer {
            emit!(self.writer, "$addr#$signer(");
        }
        self.translate_exp(&args[0]);
        if is_signer {
            emit!(self.writer, ")");
        }
        emit!(self.writer, "]");
    }

    fn with_range_selector_assignments<F>(
        &self,
        ranges: &[(LocalVarDecl, Exp)],
        range_tmps: &HashMap<Symbol, String>,
        quant_vars: &HashMap<Symbol, String>,
        resource_vars: &HashMap<Symbol, String>,
        f: F,
    ) where
        F: Fn(),
    {
        // Translate range selectors.
        for (var, range) in ranges {
            let var_name = self.env.symbol_pool().string(var.name);
            let quant_ty = self.get_node_type(range.node_id());
            match quant_ty.skip_reference() {
                Type::Vector(_) => {
                    let range_tmp = range_tmps.get(&var.name).unwrap();
                    let quant_var = quant_vars.get(&var.name).unwrap();
                    emit!(
                        self.writer,
                        "(var {} := ReadVec({}, {});\n",
                        var_name,
                        range_tmp,
                        quant_var,
                    );
                }
                Type::Primitive(PrimitiveType::Range) => {
                    let quant_var = quant_vars.get(&var.name).unwrap();
                    emit!(self.writer, "(var {} := {};\n", var_name, quant_var);
                }
                Type::ResourceDomain(mid, sid, inst_opt) => {
                    let memory = &mid.qualified_inst(*sid, inst_opt.to_owned().unwrap_or_default());
                    let addr_var = resource_vars.get(&var.name).unwrap();
                    let resource_name = boogie_resource_memory_name(self.env, memory, &None);
                    emit!(
                        self.writer,
                        "(var {} := $ResourceValue({}, {});\n",
                        var_name,
                        resource_name,
                        addr_var
                    );
                }
                _ => (),
            }
        }
        f();
        emit!(
            self.writer,
            &")".repeat(usize::checked_add(range_tmps.len(), resource_vars.len()).unwrap())
        );
    }

    fn translate_quant(
        &self,
        _node_id: NodeId,
        kind: QuantKind,
        ranges: &[(LocalVarDecl, Exp)],
        triggers: &[Vec<Exp>],
        condition: &Option<Exp>,
        body: &Exp,
    ) {
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        assert!(!kind.is_choice());
        // Translate range expressions. While doing, check for currently unsupported
        // type quantification
        let mut range_tmps = HashMap::new();
        for (var, range) in ranges {
            let should_bind_range = match self.get_node_type(range.node_id()).skip_reference() {
                Type::Vector(..) | Type::Primitive(PrimitiveType::Range) => true,
                Type::Struct(mid, sid, ..) => {
                    let struct_env = self.env.get_struct(mid.qualified(*sid));
                    struct_env.is_intrinsic_of(INTRINSIC_TYPE_MAP)
                }
                Type::Primitive(_)
                | Type::Tuple(_)
                | Type::TypeParameter(_)
                | Type::Reference(_, _)
                | Type::Fun(_, _)
                | Type::TypeDomain(_)
                | Type::ResourceDomain(_, _, _)
                | Type::Error
                | Type::Var(_) => false,
            };
            if should_bind_range {
                let range_tmp = self.fresh_var_name("range");
                emit!(self.writer, "(var {} := ", range_tmp);
                self.translate_exp(range);
                emit!(self.writer, "; ");
                range_tmps.insert(var.name, range_tmp);
            }
        }
        // Translate quantified variables.
        emit!(self.writer, "({} ", kind);
        let mut quant_vars = HashMap::new();
        let mut resource_vars = HashMap::new();
        let mut comma = "";
        for (var, range) in ranges {
            let var_name = self.env.symbol_pool().string(var.name);
            let quant_ty = self.get_node_type(range.node_id());
            let num_oper = global_state.get_node_num_oper(range.node_id());
            let ty_str = |ty: _| {
                if num_oper == Bitwise {
                    boogie_bv_type(self.env, ty)
                } else {
                    boogie_type(self.env, ty)
                }
            };
            match quant_ty.skip_reference() {
                Type::TypeDomain(ty) => {
                    emit!(self.writer, "{}{}: {}", comma, var_name, ty_str(ty));
                }
                Type::Struct(mid, sid, targs) => {
                    let struct_env = self.env.get_struct(mid.qualified(*sid));
                    if struct_env.is_intrinsic_of(INTRINSIC_TYPE_MAP) {
                        emit!(self.writer, "{}{}: {}", comma, var_name, ty_str(&targs[0]));
                    } else {
                        panic!("unexpected type");
                    }
                }
                Type::ResourceDomain(..) => {
                    let addr_quant_var = self.fresh_var_name("a");
                    emit!(self.writer, "{}{}: int", comma, addr_quant_var);
                    resource_vars.insert(var.name, addr_quant_var);
                }
                _ => {
                    let quant_var = self.fresh_var_name("i");
                    emit!(self.writer, "{}{}: int", comma, quant_var);
                    quant_vars.insert(var.name, quant_var);
                }
            }
            comma = ", ";
        }
        emit!(self.writer, " :: ");
        // Translate triggers.
        if !triggers.is_empty() {
            for trigger in triggers {
                emit!(self.writer, "{");
                let mut comma = "";
                for p in trigger {
                    emit!(self.writer, "{}", comma);
                    self.with_range_selector_assignments(
                        ranges,
                        &range_tmps,
                        &quant_vars,
                        &resource_vars,
                        || {
                            self.translate_exp(p);
                        },
                    );
                    comma = ",";
                }
                emit!(self.writer, "}");
            }
        } else {
            // Implicit triggers from ResourceDomain range.
            for (var, range) in ranges {
                let quant_ty = self.get_node_type(range.node_id());
                if let Type::ResourceDomain(mid, sid, inst_opt) = quant_ty.skip_reference() {
                    let addr_var = resource_vars.get(&var.name).unwrap();
                    let memory = &mid.qualified_inst(*sid, inst_opt.to_owned().unwrap_or_default());
                    let resource_name = boogie_resource_memory_name(self.env, memory, &None);
                    let resource_value = format!("$ResourceValue({}, {})", resource_name, addr_var);
                    emit!(self.writer, "{{{}}}", resource_value);
                }
            }
        }
        // Translate range constraints.
        let connective = match kind {
            QuantKind::Forall => " ==> ",
            QuantKind::Exists => " && ",
            _ => unreachable!(),
        };
        let mut separator = "";
        for (var, range) in ranges {
            let var_name = self.env.symbol_pool().string(var.name);
            let quant_ty = self.get_node_type(range.node_id());
            let num_oper = global_state.get_node_num_oper(range.node_id());
            match quant_ty.skip_reference() {
                Type::TypeDomain(domain_ty) => {
                    let mut type_check = boogie_well_formed_expr(self.env, &var_name, domain_ty);
                    if type_check.is_empty() {
                        type_check = "true".to_string();
                    }
                    emit!(self.writer, "{}{}", separator, type_check);
                }
                Type::ResourceDomain(..) => {
                    // currently does not generate a constraint
                    continue;
                }
                Type::Vector(..) => {
                    let range_tmp = range_tmps.get(&var.name).unwrap();
                    let quant_var = quant_vars.get(&var.name).unwrap();
                    emit!(
                        self.writer,
                        "{}InRangeVec({}, {})",
                        separator,
                        range_tmp,
                        quant_var,
                    );
                }
                Type::Struct(mid, sid, targs) => {
                    let struct_env = self.env.get_struct(mid.qualified(*sid));
                    if struct_env.is_intrinsic_of(INTRINSIC_TYPE_MAP) {
                        emit!(
                            self.writer,
                            "{}ContainsTable({}, $EncodeKey'{}'({}))",
                            separator,
                            range_tmps.get(&var.name).unwrap(),
                            boogie_type_suffix_bv(self.env, &targs[0], num_oper == Bitwise),
                            var_name,
                        );
                    } else {
                        panic!("unexpected type");
                    }
                }
                Type::Primitive(PrimitiveType::Range) => {
                    let range_tmp = range_tmps.get(&var.name).unwrap();
                    let quant_var = quant_vars.get(&var.name).unwrap();
                    emit!(
                        self.writer,
                        "{}$InRange({}, {})",
                        separator,
                        range_tmp,
                        quant_var,
                    );
                }
                Type::Primitive(_)
                | Type::Tuple(_)
                | Type::TypeParameter(_)
                | Type::Reference(_, _)
                | Type::Fun(_, _)
                | Type::Error
                | Type::Var(_) => panic!("unexpected type"),
            }
            separator = connective;
        }
        emit!(self.writer, "{}", separator);
        self.with_range_selector_assignments(
            ranges,
            &range_tmps,
            &quant_vars,
            &resource_vars,
            || {
                // Translate body and "where" condition.
                if let Some(cond) = condition {
                    emit!(self.writer, "(");
                    self.translate_exp(cond);
                    emit!(self.writer, ") {}", connective);
                }
                emit!(self.writer, "(");
                self.translate_exp(body);
                emit!(self.writer, ")");
            },
        );
        emit!(
            self.writer,
            &")".repeat(quant_vars.len().checked_add(1).unwrap())
        );
    }

    /// Translate a `some x: T: P[x]` expression. This saves information about the axiomatized
    /// function representing this expression, to be generated later, and replaces the expression by
    /// a call to this function.
    fn translate_choice(
        &self,
        node_id: NodeId,
        kind: QuantKind,
        range: &(LocalVarDecl, Exp),
        body: &Exp,
    ) {
        // Reconstruct the choice so we can easily determine used locals and temps.
        let range_and_body = ExpData::Quant(
            node_id,
            kind,
            vec![range.clone()],
            vec![],
            None,
            body.clone(),
        );
        let some_var = range.0.name;
        let free_vars = range_and_body
            .free_vars(self.env)
            .into_iter()
            .filter(|(s, _)| *s != some_var)
            .map(|(s, ty)| (s, self.inst(ty.skip_reference())))
            .collect_vec();
        let used_temps = range_and_body
            .used_temporaries(self.env)
            .into_iter()
            .collect_vec();
        let used_memory = range_and_body
            .used_memory(self.env)
            .into_iter()
            .collect_vec();

        // Create a new uninterpreted function and choice info only if it does not
        // stem from the same original source than an existing one. This needs to be done to
        // avoid non-determinism in reasoning with choices resulting from duplication
        // of the same expressions. Consider a user has written `ensures choose i: ..`.
        // This expression might be duplicated many times e.g. via opaque function caller
        // sites. We want that the choice consistently returns the same value in each case;
        // we can only guarantee this if we use the same uninterpreted function for each instance.
        // We also need to consider the type instantiation.
        // As a result, (ExpData, Vec<Type>) is used as the key
        let choice_infos_key_pair = (range_and_body, self.type_inst.clone());
        let mut choice_infos = self.lifted_choice_infos.borrow_mut();
        let choice_count = choice_infos.len();
        let info = choice_infos
            .entry(choice_infos_key_pair)
            .or_insert_with(|| LiftedChoiceInfo {
                id: choice_count,
                node_id,
                kind,
                free_vars: free_vars.clone(),
                used_temps: used_temps.clone(),
                used_memory: used_memory.clone(),
                var: some_var,
                range: range.1.clone(),
                condition: body.clone(),
            });
        let fun_name = boogie_choice_fun_name(info.id);

        // Construct the arguments. Notice that those might be different for each call of
        // the choice function, resulting from the choice being injected into multiple contexts
        // with different substitutions.
        let args = free_vars
            .iter()
            .map(|(s, _)| s.display(self.env.symbol_pool()).to_string())
            .chain(used_temps.iter().map(|(t, _)| format!("$t{}", t)))
            .chain(
                used_memory
                    .iter()
                    .map(|(m, l)| boogie_resource_memory_name(self.env, m, l)),
            )
            .join(", ");
        emit!(self.writer, "{}({})", fun_name, args);
    }

    fn translate_eq_neq(&self, boogie_val_fun: &str, args: &[Exp]) {
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let num_oper = global_state.get_node_num_oper(args[0].node_id());
        let suffix = boogie_type_suffix_bv(
            self.env,
            self.get_node_type(args[0].node_id()).skip_reference(),
            num_oper == Bitwise,
        );
        emit!(self.writer, "{}'{}'(", boogie_val_fun, suffix);
        self.translate_exp(&args[0]);
        emit!(self.writer, ", ");
        self.translate_exp(&args[1]);
        emit!(self.writer, ")");
    }

    fn translate_identical(&self, args: &[Exp]) {
        use ExpData::*;
        // If both arguments are &mut temporaries, we just directly make them equal. This allows
        // a more efficient representation of equality between $Mutation objects. Otherwise
        // we translate it the default way with automatic reference removal.
        match (&args[0].as_ref(), &args[1].as_ref()) {
            (Temporary(id1, idx1), Temporary(id2, idx2))
                if self.get_node_type(*id1).is_reference()
                    && self.get_node_type(*id2).is_reference() =>
            {
                emit!(self.writer, "$t{} == $t{}", idx1, idx2);
            }
            _ => self.translate_rel_op("==", args),
        }
    }

    fn translate_op(&self, boogie_op: &str, bv_op: &str, args: &[Exp]) {
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let num_oper = global_state.get_node_num_oper(args[0].node_id());
        if num_oper == Bitwise {
            let oper_base = match self.env.get_node_type(args[0].node_id()).skip_reference() {
                Type::Primitive(PrimitiveType::U8) => "Bv8",
                Type::Primitive(PrimitiveType::U16) => "Bv16",
                Type::Primitive(PrimitiveType::U32) => "Bv32",
                Type::Primitive(PrimitiveType::U64) => "Bv64",
                Type::Primitive(PrimitiveType::U128) => "Bv128",
                Type::Primitive(PrimitiveType::U256) => "Bv256",
                Type::Primitive(PrimitiveType::Num) => "<<num is not unsupported here>>",
                _ => unreachable!(),
            };
            emit!(self.writer, "${}'{}'(", bv_op, oper_base);
            self.translate_seq(args.iter(), ", ", |e| self.translate_exp(e));
            emit!(self.writer, ")");
        } else {
            emit!(self.writer, "(");
            self.translate_exp(&args[0]);
            emit!(self.writer, " {} ", boogie_op);
            self.translate_exp(&args[1]);
            emit!(self.writer, ")");
        }
    }

    fn translate_bit_op(&self, boogie_op: &str, args: &[Exp]) {
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let oper_base = match self.env.get_node_type(args[0].node_id()).skip_reference() {
            Type::Primitive(PrimitiveType::U8) => "Bv8",
            Type::Primitive(PrimitiveType::U16) => "Bv16",
            Type::Primitive(PrimitiveType::U32) => "Bv32",
            Type::Primitive(PrimitiveType::U64) => "Bv64",
            Type::Primitive(PrimitiveType::U128) => "Bv128",
            Type::Primitive(PrimitiveType::U256) => "Bv256",
            Type::Primitive(PrimitiveType::Num) => "<<num is not unsupported here>>",
            _ => unreachable!(),
        };
        emit!(self.writer, "{}'{}'(", boogie_op, oper_base);
        self.translate_seq(args.iter(), ", ", |e| {
            let num_oper_e = global_state.get_node_num_oper(e.node_id());
            let ty_e = self.env.get_node_type(e.node_id());
            if num_oper_e != Bitwise {
                emit!(self.writer, "$int2bv.{}(", boogie_num_type_base(&ty_e));
            }
            self.translate_exp(e);
            if num_oper_e != Bitwise {
                emit!(self.writer, ")")
            }
        });
        emit!(self.writer, ")");
    }

    fn translate_rel_op(&self, boogie_op: &str, args: &[Exp]) {
        emit!(self.writer, "(");
        self.translate_exp(&args[0]);
        emit!(self.writer, " {} ", boogie_op);
        self.translate_exp(&args[1]);
        emit!(self.writer, ")");
    }

    fn translate_logical_op(&self, boogie_op: &str, args: &[Exp]) {
        emit!(self.writer, "(");
        self.translate_exp(&args[0]);
        emit!(self.writer, " {} ", boogie_op);
        self.translate_exp(&args[1]);
        emit!(self.writer, ")");
    }

    fn translate_logical_unary_op(&self, boogie_op: &str, args: &[Exp]) {
        emit!(self.writer, "{}", boogie_op);
        self.translate_exp(&args[0]);
    }

    fn translate_cast(&self, node_id: NodeId, args: &[Exp]) {
        let mut global_state = self
            .env
            .get_cloned_extension::<GlobalNumberOperationState>();
        let arg = args[0].clone();
        self.env
            .update_node_type(arg.node_id(), self.env.get_node_type(node_id));
        let cast_oper = global_state.get_node_num_oper(node_id);
        global_state.update_node_oper(args[0].node_id(), cast_oper, true);
        self.env.set_extension(global_state);
        self.translate_exp(&arg);
    }

    fn translate_primitive_call(&self, fun: &str, args: &[Exp]) {
        emit!(self.writer, "{}(", fun);
        self.translate_seq(args.iter(), ", ", |e| self.translate_exp(e));
        emit!(self.writer, ")");
    }

    fn translate_primitive_call_shr(&self, fun: &str, args: &[Exp]) {
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let num_oper = global_state.get_node_num_oper(args[0].node_id());
        if num_oper == Bitwise {
            let oper_left_base = match self.env.get_node_type(args[0].node_id()).skip_reference() {
                Type::Primitive(PrimitiveType::U8) => "Bv8",
                Type::Primitive(PrimitiveType::U16) => "Bv16",
                Type::Primitive(PrimitiveType::U32) => "Bv32",
                Type::Primitive(PrimitiveType::U64) => "Bv64",
                Type::Primitive(PrimitiveType::U128) => "Bv128",
                Type::Primitive(PrimitiveType::U256) => "Bv256",
                Type::Primitive(PrimitiveType::Num) => "<<num is not unsupported here>>",
                _ => unreachable!(),
            };
            let oper_right_base = boogie_num_type_base(&self.env.get_node_type(args[1].node_id()));
            emit!(
                self.writer,
                "{}{}From{}(",
                fun,
                oper_left_base,
                oper_right_base
            );
        } else {
            emit!(self.writer, "{}(", fun);
        }
        self.translate_seq(args.iter(), ", ", |e| self.translate_exp(e));
        emit!(self.writer, ")");
    }

    fn translate_primitive_call_shl(&self, fun: &str, args: &[Exp]) {
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number operation state");
        let num_oper = global_state.get_node_num_oper(args[0].node_id());
        if num_oper == Bitwise {
            let oper_left_base = match self.env.get_node_type(args[0].node_id()).skip_reference() {
                Type::Primitive(PrimitiveType::U8) => "Bv8",
                Type::Primitive(PrimitiveType::U16) => "Bv16",
                Type::Primitive(PrimitiveType::U32) => "Bv32",
                Type::Primitive(PrimitiveType::U64) => "Bv64",
                Type::Primitive(PrimitiveType::U128) => "Bv128",
                Type::Primitive(PrimitiveType::U256) => "Bv256",
                Type::Primitive(PrimitiveType::Num) => "<<num is not unsupported here>>",
                _ => unreachable!(),
            };
            let oper_right_base = boogie_num_type_base(&self.env.get_node_type(args[1].node_id()));
            emit!(
                self.writer,
                "{}{}From{}(",
                fun,
                oper_left_base,
                oper_right_base
            );
        } else {
            let ty = self.get_node_type(args[0].node_id());
            let fun_num = match ty {
                Type::Primitive(PrimitiveType::U8) => "U8",
                Type::Primitive(PrimitiveType::U16) => "U16",
                Type::Primitive(PrimitiveType::U32) => "U32",
                Type::Primitive(PrimitiveType::U64) => "U64",
                Type::Primitive(PrimitiveType::U128) => "U128",
                Type::Primitive(PrimitiveType::U256) => "U256",
                Type::Primitive(PrimitiveType::Num) => "",
                _ => unreachable!(),
            };
            emit!(self.writer, "{}(", format!("{}{}", fun, fun_num).as_str());
        }
        self.translate_seq(args.iter(), ", ", |e| self.translate_exp(e));
        emit!(self.writer, ")");
    }

    fn translate_primitive_inst_call(&self, node_id: NodeId, fun: &str, args: &[Exp]) {
        let suffix = boogie_inst_suffix(self.env, &self.get_node_instantiation(node_id));
        emit!(self.writer, "{}{}(", fun, suffix);
        self.translate_seq(args.iter(), ", ", |e| self.translate_exp(e));
        emit!(self.writer, ")");
    }

    fn translate_well_formed(&self, exp: &Exp) {
        let global_state = &self
            .env
            .get_extension::<GlobalNumberOperationState>()
            .expect("global number state");
        let ty = self.get_node_type(exp.node_id());
        let bv_flag = global_state.get_node_num_oper(exp.node_id()) == Bitwise;
        match exp.as_ref() {
            ExpData::Temporary(_, idx) => {
                // For the special case of a temporary which can represent a
                // &mut, skip the normal translation of `exp` which would do automatic
                // dereferencing. Instead let boogie_well_formed_expr handle the
                // the dereferencing as part of its logic.
                let check =
                    boogie_well_formed_expr_bv(self.env, &format!("$t{}", idx), &ty, bv_flag);
                if !check.is_empty() {
                    emit!(self.writer, &check);
                } else {
                    emit!(self.writer, "true");
                }

                if let Type::Primitive(PrimitiveType::Signer) = ty {
                    let name = &format!("$t{}", idx);
                    let target = if ty.is_reference() {
                        format!("$Dereference({})", name)
                    } else {
                        name.to_owned()
                    };
                    emit!(
                        self.writer,
                        &format!(" && $1_signer_is_txn_signer({})", target)
                    );
                    emit!(
                        self.writer,
                        &format!(
                            " && $1_signer_is_txn_signer_addr($addr#$signer({}))",
                            target
                        )
                    );
                }
            }
            ExpData::LocalVar(_, sym) => {
                // For specification locals (which never can be references) directly emit them.
                let check = boogie_well_formed_expr_bv(
                    self.env,
                    self.env.symbol_pool().string(*sym).as_str(),
                    &ty,
                    bv_flag,
                );
                emit!(self.writer, &check);
            }
            _ => {
                let check =
                    boogie_well_formed_expr_bv(self.env, "$val", ty.skip_reference(), bv_flag);
                emit!(self.writer, "(var $val := ");
                self.translate_exp(exp);
                emit!(self.writer, "; {})", check);
            }
        }
    }
}
