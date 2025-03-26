// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, LinkedList},
};

use itertools::Itertools;
use num::{BigInt, BigUint, FromPrimitive, Zero};

use move_compiler::{
    expansion::ast as EA, hlir::ast as HA, naming::ast as NA, parser::ast as PA, shared::Name,
};
use move_core_types::runtime_value::MoveValue;
use move_ir_types::location::Spanned;

use crate::{
    ast::{Exp, ExpData, LocalVarDecl, ModuleName, Operation, QualifiedSymbol, QuantKind, Value},
    builder::{
        model_builder::{ConstEntry, DatatypeData, LocalVarEntry},
        module_builder::ModuleBuilder,
    },
    model::{DatatypeId, FieldId, Loc, ModuleId, NodeId, QualifiedId},
    symbol::{Symbol, SymbolPool},
    ty::{PrimitiveType, Substitution, Type, TypeDisplayContext, Variance, BOOL_TYPE},
};

#[derive(Debug)]
pub(crate) struct ExpTranslator<'env, 'translator, 'module_translator> {
    pub parent: &'module_translator mut ModuleBuilder<'env, 'translator>,
    /// A symbol table for type parameters.
    pub type_params_table: BTreeMap<Symbol, Type>,
    /// Type parameters in sequence they have been added.
    pub type_params: Vec<(Symbol, Type)>,
    /// A scoped symbol table for local names. The first element in the list contains the most
    /// inner scope.
    pub local_table: LinkedList<BTreeMap<Symbol, LocalVarEntry>>,
    /// When compiling a condition, the result type of the function the condition is associated
    /// with.
    #[allow(unused)]
    pub result_type: Option<Type>,
    /// Status for the `old(...)` expression form.
    pub old_status: OldExpStatus,
    /// The currently build type substitution.
    pub subs: Substitution,
    /// A counter for generating type variables.
    pub type_var_counter: u16,
    /// A marker to indicate the node_counter start state.
    pub node_counter_start: usize,
    /// The locals which have been accessed with this build. The boolean indicates whether
    /// they ore accessed in `old(..)` context.
    pub accessed_locals: BTreeSet<(Symbol, bool)>,
    /// The number of outer context scopes in  `local_table` which are accounted for in
    /// `accessed_locals`. See also documentation of function `mark_context_scopes`.
    pub outer_context_scopes: usize,
    /// A flag to indicate whether we are translating expressions in a spec fun.
    pub translating_fun_as_spec_fun: bool,
    /// A flag to indicate whether errors have been generated so far.
    pub errors_generated: RefCell<bool>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum OldExpStatus {
    NotSupported,
    OutsideOld,
    InsideOld,
}

/// # General

impl<'env, 'translator, 'module_translator> ExpTranslator<'env, 'translator, 'module_translator> {
    pub fn new(parent: &'module_translator mut ModuleBuilder<'env, 'translator>) -> Self {
        let node_counter_start = parent.parent.env.next_free_node_number();
        Self {
            parent,
            type_params_table: BTreeMap::new(),
            type_params: vec![],
            local_table: LinkedList::new(),
            result_type: None,
            old_status: OldExpStatus::NotSupported,
            subs: Substitution::new(),
            type_var_counter: 0,
            node_counter_start,
            accessed_locals: BTreeSet::new(),
            outer_context_scopes: 0,
            /// Following flags used to translate pure Move functions.
            translating_fun_as_spec_fun: false,
            errors_generated: RefCell::new(false),
        }
    }

    pub fn new_with_old(
        parent: &'module_translator mut ModuleBuilder<'env, 'translator>,
        allow_old: bool,
    ) -> Self {
        let mut et = ExpTranslator::new(parent);
        if allow_old {
            et.old_status = OldExpStatus::OutsideOld;
        } else {
            et.old_status = OldExpStatus::NotSupported;
        };
        et
    }

    pub fn translate_fun_as_spec_fun(&mut self) {
        self.translating_fun_as_spec_fun = true;
    }

    /// Extract a map from names to types from the scopes of this build.
    pub fn extract_var_map(&self) -> BTreeMap<Symbol, LocalVarEntry> {
        let mut vars: BTreeMap<Symbol, LocalVarEntry> = BTreeMap::new();
        for s in &self.local_table {
            vars.extend(s.clone());
        }
        vars
    }

    // Get type parameters from this build.
    #[allow(unused)]
    pub fn get_type_params(&self) -> Vec<Type> {
        self.type_params
            .iter()
            .map(|(_, t)| t.clone())
            .collect_vec()
    }

    // Get type parameters with names from this build.
    pub fn get_type_params_with_name(&self) -> Vec<(Symbol, Type)> {
        self.type_params.clone()
    }

    /// Shortcut for accessing symbol pool.
    pub fn symbol_pool(&self) -> &SymbolPool {
        self.parent.parent.env.symbol_pool()
    }

    /// Shortcut for translating a Move AST location into ours.
    pub fn to_loc(&self, loc: &move_ir_types::location::Loc) -> Loc {
        self.parent.parent.env.to_loc(loc)
    }

    /// Shortcut for reporting an error.
    pub fn error(&self, loc: &Loc, msg: &str) {
        if self.translating_fun_as_spec_fun {
            *self.errors_generated.borrow_mut() = true;
        } else {
            self.parent.parent.error(loc, msg);
        }
    }

    /// Creates a fresh type variable.
    fn fresh_type_var(&mut self) -> Type {
        let var = Type::Var(self.type_var_counter);
        self.type_var_counter += 1;
        var
    }

    /// Shortcut to create a new node id.
    fn new_node_id(&self) -> NodeId {
        self.parent.parent.env.new_node_id()
    }

    /// Shortcut to create a new node id and assigns type and location to it.
    pub fn new_node_id_with_type_loc(&self, ty: &Type, loc: &Loc) -> NodeId {
        self.parent.parent.env.new_node(loc.clone(), ty.clone())
    }

    // Short cut for getting node type.
    pub fn get_node_type(&self, node_id: NodeId) -> Type {
        self.parent.parent.env.get_node_type(node_id)
    }

    // Short cut for getting node type.
    fn get_node_type_opt(&self, node_id: NodeId) -> Option<Type> {
        self.parent.parent.env.get_node_type_opt(node_id)
    }

    // Short cut for getting node location.
    #[allow(dead_code)]
    fn get_node_loc(&self, node_id: NodeId) -> Loc {
        self.parent.parent.env.get_node_loc(node_id)
    }

    // Short cut for getting node instantiation.
    fn get_node_instantiation_opt(&self, node_id: NodeId) -> Option<Vec<Type>> {
        self.parent.parent.env.get_node_instantiation_opt(node_id)
    }

    /// Shortcut to update node type.
    pub fn update_node_type(&self, node_id: NodeId, ty: Type) {
        self.parent.parent.env.update_node_type(node_id, ty);
    }

    /// Shortcut to set/update instantiation for the given node id.
    fn set_node_instantiation(&self, node_id: NodeId, instantiation: Vec<Type>) {
        self.parent
            .parent
            .env
            .set_node_instantiation(node_id, instantiation);
    }

    fn update_node_instantiation(&self, node_id: NodeId, instantiation: Vec<Type>) {
        self.parent
            .parent
            .env
            .update_node_instantiation(node_id, instantiation);
    }

    /// Finalizes types in this build, producing errors if some could not be inferred
    /// and remained incomplete.
    pub fn finalize_types(&mut self) {
        for i in self.node_counter_start..self.parent.parent.env.next_free_node_number() {
            let node_id = NodeId::new(i);

            if let Some(ty) = self.get_node_type_opt(node_id) {
                let ty = self.finalize_type(node_id, &ty);
                self.update_node_type(node_id, ty);
            }
            if let Some(inst) = self.get_node_instantiation_opt(node_id) {
                let inst = inst
                    .iter()
                    .map(|ty| self.finalize_type(node_id, ty))
                    .collect_vec();
                self.update_node_instantiation(node_id, inst);
            }
        }
    }

    /// Finalize the the given type, producing an error if it is not complete.
    fn finalize_type(&self, node_id: NodeId, ty: &Type) -> Type {
        let ty = self.subs.specialize(ty);
        if ty.is_incomplete() {
            // This type could not be fully inferred.
            let loc = self.parent.parent.env.get_node_loc(node_id);
            self.error(
                &loc,
                &format!(
                    "unable to infer type: `{}`",
                    ty.display(&self.type_display_context())
                ),
            );
        }
        ty
    }

    /// Fix any free type variables remaining in this expression build to a freshly
    /// generated type parameter, adding them to the passed vector.
    #[allow(unused)]
    pub fn fix_types(&mut self, generated_params: &mut Vec<Type>) {
        if self.parent.parent.env.has_errors() {
            return;
        }
        for i in self.node_counter_start..self.parent.parent.env.next_free_node_number() {
            let node_id = NodeId::new(i);

            if let Some(ty) = self.get_node_type_opt(node_id) {
                let ty = self.fix_type(generated_params, &ty);
                self.update_node_type(node_id, ty);
            }
            if let Some(inst) = self.get_node_instantiation_opt(node_id) {
                let inst = inst
                    .iter()
                    .map(|ty| self.fix_type(generated_params, ty))
                    .collect_vec();
                self.update_node_instantiation(node_id, inst);
            }
        }
    }

    /// Fix the given type, replacing any remaining free type variables with a type parameter.
    fn fix_type(&mut self, generated_params: &mut Vec<Type>, ty: &Type) -> Type {
        // First specialize the type.
        let ty = self.subs.specialize(ty);
        // Next get whatever free variables remain.
        let vars = ty.get_vars();
        // Assign a type parameter to each free variable and add it to substitution.
        for var in vars {
            let type_param = Type::TypeParameter(generated_params.len() as u16);
            generated_params.push(type_param.clone());
            self.subs.bind(var, type_param);
        }
        // Return type with type parameter substitution applied.
        self.subs.specialize(&ty)
    }

    /// Constructs a type display context used to visualize types in error messages.
    fn type_display_context(&self) -> TypeDisplayContext<'_> {
        TypeDisplayContext::WithoutEnv {
            symbol_pool: self.symbol_pool(),
            reverse_datatype_table: &self.parent.parent.reverse_datatype_table,
        }
    }

    /// Creates an error expression.
    pub fn new_error_exp(&mut self) -> ExpData {
        let id =
            self.new_node_id_with_type_loc(&Type::Error, &self.parent.parent.env.internal_loc());
        ExpData::Invalid(id)
    }

    /// Enters a new scope in the locals table.
    pub fn enter_scope(&mut self) {
        self.local_table.push_front(BTreeMap::new());
    }

    /// Exits the most inner scope of the locals table.
    pub fn exit_scope(&mut self) {
        self.local_table.pop_front();
    }

    /// Mark the current active scope level as context, i.e. symbols which are not
    /// declared in this expression. This is used to determine what
    /// `get_accessed_context_locals` returns.
    #[allow(unused)]
    pub fn mark_context_scopes(mut self) -> Self {
        self.outer_context_scopes = self.local_table.len();
        self
    }

    /// Gets the locals this build has accessed so far and which belong to the
    /// context, i.a. are not declared in this expression.
    #[allow(unused)]
    pub fn get_accessed_context_locals(&self) -> Vec<(Symbol, bool)> {
        self.accessed_locals.iter().cloned().collect_vec()
    }

    /// Defines a type parameter.
    pub fn define_type_param(&mut self, loc: &Loc, name: Symbol, ty: Type) {
        if let Type::TypeParameter(..) = &ty {
            if self.type_params_table.insert(name, ty.clone()).is_some() {
                let param_name = name.display(self.symbol_pool());
                self.parent.parent.error(
                    loc,
                    &format!(
                        "duplicate declaration of type parameter `{}`, \
                        previously found in type parameters",
                        param_name
                    ),
                );
                return;
            }
            self.type_params.push((name, ty));
        } else {
            let param_name = name.display(self.symbol_pool());
            let context = TypeDisplayContext::WithEnv {
                env: self.parent.parent.env,
                type_param_names: None,
            };
            self.parent.parent.error(
                loc,
                &format!(
                    "expect type placeholder `{}` to be a `TypeParameter`, found `{}`",
                    param_name,
                    ty.display(&context)
                ),
            );
        }
    }

    /// Defines a local in the most inner scope. This produces an error
    /// if the name already exists. The operation option is used for names
    /// which represent special operations.
    pub fn define_local(
        &mut self,
        loc: &Loc,
        name: Symbol,
        type_: Type,
        operation: Option<Operation>,
        temp_index: Option<usize>,
    ) {
        self.internal_define_local(loc, name, type_, operation, temp_index)
    }

    /// Defines a let local.
    pub fn define_let_local(&mut self, loc: &Loc, name: Symbol, type_: Type) {
        self.internal_define_local(loc, name, type_, None, None)
    }

    fn internal_define_local(
        &mut self,
        loc: &Loc,
        name: Symbol,
        type_: Type,
        operation: Option<Operation>,
        temp_index: Option<usize>,
    ) {
        let entry = LocalVarEntry {
            loc: loc.clone(),
            type_,
            operation,
            temp_index,
        };
        if let Some(old) = self
            .local_table
            .front_mut()
            .expect("symbol table empty")
            .insert(name, entry)
        {
            let display = name.display(self.symbol_pool());
            self.error(loc, &format!("duplicate declaration of `{}`", display));
            self.error(&old.loc, &format!("previous declaration of `{}`", display));
        }
    }

    /// Lookup a local in this build.
    pub fn lookup_local(&mut self, name: Symbol, in_old: bool) -> Option<&LocalVarEntry> {
        let mut depth = self.local_table.len();
        for scope in &self.local_table {
            if let Some(entry) = scope.get(&name) {
                if depth <= self.outer_context_scopes {
                    // Account for access if this belongs to one of the outer scopes
                    // considered context (i.e. not declared in this expression).
                    self.accessed_locals.insert((name, in_old));
                }
                return Some(entry);
            }
            depth -= 1;
        }
        None
    }

    /// Analyzes the sequence of type parameters as they are provided via the source AST and enters
    /// them into the environment. Returns a vector for representing them in the target AST.
    pub fn analyze_and_add_type_params<'a, I>(&mut self, type_params: I) -> Vec<(Symbol, Type)>
    where
        I: IntoIterator<Item = &'a Name>,
    {
        type_params
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                let ty = Type::TypeParameter(i as u16);
                let sym = self.symbol_pool().make(n.value.as_str());
                self.define_type_param(&self.to_loc(&n.loc), sym, ty.clone());
                (sym, ty)
            })
            .collect_vec()
    }

    /// Analyzes the sequence of function parameters as they are provided via the source AST and
    /// enters them into the environment. Returns a vector for representing them in the target AST.
    pub fn analyze_and_add_params(
        &mut self,
        params: &[(EA::Mutability, PA::Var, EA::Type)],
        for_move_fun: bool,
    ) -> Vec<(Symbol, Type)> {
        params
            .iter()
            .enumerate()
            .map(|(idx, (_, v, ty))| {
                let ty = self.translate_type(ty);
                let sym = self.symbol_pool().make(v.0.value.as_str());
                self.define_local(
                    &self.to_loc(&v.0.loc),
                    sym,
                    ty.clone(),
                    None,
                    // If this is for a proper Move function (not spec function), add the
                    // index so we can resolve this to a `Temporary` expression instead of
                    // a `LocalVar`.
                    if for_move_fun { Some(idx) } else { None },
                );
                (sym, ty)
            })
            .collect_vec()
    }
}

/// # Type Translation

impl<'env, 'translator, 'module_translator> ExpTranslator<'env, 'translator, 'module_translator> {
    /// Translates a source AST type into a target AST type.
    pub fn translate_type(&mut self, ty: &EA::Type) -> Type {
        use EA::Type_::*;
        match &ty.value {
            Apply(access, args) => {
                if let EA::ModuleAccess_::Name(n) = &access.value {
                    let check_zero_args = |et: &mut Self, ty: Type| {
                        if args.is_empty() {
                            ty
                        } else {
                            et.error(&et.to_loc(&n.loc), "expected no type arguments");
                            Type::Error
                        }
                    };
                    // Attempt to resolve as builtin type.
                    match n.value.as_str() {
                        "bool" => {
                            return check_zero_args(self, Type::new_prim(PrimitiveType::Bool));
                        }
                        "u8" => return check_zero_args(self, Type::new_prim(PrimitiveType::U8)),
                        "u16" => return check_zero_args(self, Type::new_prim(PrimitiveType::U16)),
                        "u32" => return check_zero_args(self, Type::new_prim(PrimitiveType::U32)),
                        "u64" => return check_zero_args(self, Type::new_prim(PrimitiveType::U64)),
                        "u128" => {
                            return check_zero_args(self, Type::new_prim(PrimitiveType::U128));
                        }
                        "u256" => {
                            return check_zero_args(self, Type::new_prim(PrimitiveType::U256))
                        }
                        "num" => return check_zero_args(self, Type::new_prim(PrimitiveType::Num)),
                        "range" => {
                            return check_zero_args(self, Type::new_prim(PrimitiveType::Range));
                        }
                        "address" => {
                            return check_zero_args(self, Type::new_prim(PrimitiveType::Address));
                        }
                        "signer" => {
                            return check_zero_args(self, Type::new_prim(PrimitiveType::Signer));
                        }
                        "vector" => {
                            if args.len() != 1 {
                                self.error(
                                    &self.to_loc(&ty.loc),
                                    "expected one type argument for `vector`",
                                );
                                return Type::Error;
                            } else {
                                return Type::Vector(Box::new(self.translate_type(&args[0])));
                            }
                        }
                        _ => {}
                    }
                    // Attempt to resolve as a type parameter.
                    let sym = self.symbol_pool().make(n.value.as_str());
                    if let Some(ty) = self.type_params_table.get(&sym).cloned() {
                        return check_zero_args(self, ty);
                    }
                }
                let loc = self.to_loc(&access.loc);
                let sym = self.parent.module_access_to_qualified(access);
                let rty = self.parent.parent.lookup_type(&loc, &sym);
                // Replace type instantiation.
                if let Type::Datatype(mid, sid, params) = &rty {
                    if params.len() != args.len() {
                        self.error(&loc, "type argument count mismatch");
                        Type::Error
                    } else {
                        Type::Datatype(*mid, *sid, self.translate_types(args))
                    }
                } else if !args.is_empty() {
                    self.error(&loc, "type cannot have type arguments");
                    Type::Error
                } else {
                    rty
                }
            }
            Ref(is_mut, ty) => Type::Reference(*is_mut, Box::new(self.translate_type(ty))),
            Fun(args, result) => Type::Fun(
                self.translate_types(args),
                Box::new(self.translate_type(result)),
            ),
            Unit => Type::Tuple(vec![]),
            Multiple(vst) => Type::Tuple(self.translate_types(vst)),
            UnresolvedError => Type::Error,
        }
    }

    /// Translates a slice of single types.
    pub fn translate_types(&mut self, tys: &[EA::Type]) -> Vec<Type> {
        tys.iter().map(|t| self.translate_type(t)).collect()
    }
}

/// # Expression Translation

impl<'env, 'translator, 'module_translator> ExpTranslator<'env, 'translator, 'module_translator> {
    /// Translates an expression, with given expected type, which might be a type variable.
    pub fn translate_exp(&mut self, exp: &EA::Exp, expected_type: &Type) -> ExpData {
        let loc = self.to_loc(&exp.loc);
        let make_value = |et: &mut ExpTranslator, val: Value, ty: Type| {
            let rty = et.check_type(&loc, &ty, expected_type, "in expression");
            let id = et.new_node_id_with_type_loc(&rty, &loc);
            ExpData::Value(id, val)
        };
        match &exp.value {
            EA::Exp_::Value(v) => {
                if let Some((v, ty)) = self.translate_value(v) {
                    make_value(self, v, ty)
                } else {
                    self.new_error_exp()
                }
            }
            EA::Exp_::Pack(maccess, generics, fields) => {
                self.translate_pack(&loc, maccess, generics, fields, expected_type)
            }
            EA::Exp_::IfElse(cond, then, Some(else_)) => {
                let then = self.translate_exp(then, expected_type);
                let else_ = self.translate_exp(else_, expected_type);
                let cond = self.translate_exp(cond, &Type::new_prim(PrimitiveType::Bool));
                let id = self.new_node_id_with_type_loc(expected_type, &loc);
                ExpData::IfElse(id, cond.into(), then.into_exp(), else_.into_exp())
            }
            EA::Exp_::Block(_label, seq) => self.translate_seq(&loc, seq, expected_type),
            EA::Exp_::Lambda(bindings, _, exp) => {
                self.translate_lambda(&loc, bindings, exp, expected_type)
            }
            EA::Exp_::Quant(kind, ranges, triggers, condition, body) => self.translate_quant(
                &loc,
                *kind,
                ranges,
                triggers,
                condition,
                body,
                expected_type,
            ),
            EA::Exp_::ExpDotted(usage, dotted) => match usage {
                EA::DottedUsage::Move(_)
                | EA::DottedUsage::Copy(_)
                | EA::DottedUsage::Borrow(_) => {
                    if self.translating_fun_as_spec_fun {
                        self.translate_dotted(dotted, expected_type)
                    } else {
                        self.error(&loc, "expression construct not supported in specifications");
                        self.new_error_exp()
                    }
                }
                EA::DottedUsage::Use => self.translate_dotted(dotted, expected_type),
            },
            EA::Exp_::Index(target, index) => {
                self.translate_index(&loc, target, index, expected_type)
            }
            EA::Exp_::ExpList(exps) => {
                let mut types = vec![];
                let exps = exps
                    .iter()
                    .map(|exp| {
                        let (ty, exp) = self.translate_exp_free(exp);
                        types.push(ty);
                        exp.into_exp()
                    })
                    .collect_vec();
                let ty = self.check_type(
                    &loc,
                    &Type::Tuple(types),
                    expected_type,
                    "in expression list",
                );
                let id = self.new_node_id_with_type_loc(&ty, &loc);
                ExpData::Call(id, Operation::Tuple, exps)
            }
            EA::Exp_::Unit { .. } => {
                let ty = self.check_type(
                    &loc,
                    &Type::Tuple(vec![]),
                    expected_type,
                    "in unit expression",
                );
                let id = self.new_node_id_with_type_loc(&ty, &loc);
                ExpData::Call(id, Operation::Tuple, vec![])
            }
            EA::Exp_::Assign(..) => {
                self.error(&loc, "assignment only allowed in spec var updates");
                self.new_error_exp()
            }
            EA::Exp_::Dereference(exp) => {
                if self.translating_fun_as_spec_fun {
                    self.translate_exp(exp, expected_type)
                } else {
                    self.error(&loc, "expression construct not supported in specifications");
                    self.new_error_exp()
                }
            }
            EA::Exp_::Cast(exp, typ) => {
                let ty = self.translate_type(typ);
                self.check_type(&loc, &ty, expected_type, "in cast expression");
                let (exp_ty, exp) = self.translate_exp_free(exp);
                if !ty.is_number() || !exp_ty.is_number() {
                    self.error(&loc, "the cast target can only be num types");
                    self.new_error_exp()
                } else {
                    ExpData::Call(
                        self.new_node_id_with_type_loc(&ty, &loc),
                        Operation::Cast,
                        vec![exp.into_exp()],
                    )
                }
            }
            _ => {
                self.error(&loc, "expression construct not supported in specifications");
                self.new_error_exp()
            }
        }
    }

    pub fn translate_value(&mut self, v: &EA::Value) -> Option<(Value, Type)> {
        let loc = self.to_loc(&v.loc);
        match &v.value {
            EA::Value_::Address(addr) => {
                let addr_bytes = self.parent.parent.resolve_address(&loc, addr);
                let value = Value::Address(BigUint::from_bytes_be(&addr_bytes.into_bytes()));
                Some((value, Type::new_prim(PrimitiveType::Address)))
            }
            EA::Value_::U8(x) => Some((
                Value::Number(BigInt::from_u8(*x).unwrap()),
                Type::new_prim(PrimitiveType::U8),
            )),
            EA::Value_::U16(x) => Some((
                Value::Number(BigInt::from_u16(*x).unwrap()),
                Type::new_prim(PrimitiveType::U16),
            )),
            EA::Value_::U32(x) => Some((
                Value::Number(BigInt::from_u32(*x).unwrap()),
                Type::new_prim(PrimitiveType::U32),
            )),
            EA::Value_::U64(x) => Some((
                Value::Number(BigInt::from_u64(*x).unwrap()),
                Type::new_prim(PrimitiveType::U64),
            )),
            EA::Value_::U128(x) => Some((
                Value::Number(BigInt::from_u128(*x).unwrap()),
                Type::new_prim(PrimitiveType::U128),
            )),
            EA::Value_::InferredNum(x) | EA::Value_::U256(x) => Some((
                Value::Number(BigInt::from(x)),
                Type::new_prim(PrimitiveType::U256),
            )),
            EA::Value_::Bool(x) => Some((Value::Bool(*x), Type::new_prim(PrimitiveType::Bool))),
            EA::Value_::Bytearray(x) => {
                let ty = Type::Vector(Box::new(Type::new_prim(PrimitiveType::U8)));
                Some((Value::ByteArray(x.clone()), ty))
            }
        }
    }

    /// Translates an expression without any known type expectation. This creates a fresh type
    /// variable and passes this in as expected type, then returns a pair of this type and the
    /// translated expression.
    pub fn translate_exp_free(&mut self, exp: &EA::Exp) -> (Type, ExpData) {
        let tvar = self.fresh_type_var();
        let exp = self.translate_exp(exp, &tvar);
        (self.subs.specialize(&tvar), exp)
    }

    /// Translates a sequence expression.
    pub fn translate_seq(
        &mut self,
        loc: &Loc,
        (_, seq): &EA::Sequence,
        expected_type: &Type,
    ) -> ExpData {
        let n = seq.len();
        if n == 0 {
            self.error(loc, "block sequence cannot be empty");
            return self.new_error_exp();
        }
        // Process all items before the last one, which must be bindings, and accumulate
        // declarations for them.
        let mut decls = vec![];
        let seq = seq.iter().collect_vec();
        for item in &seq[0..seq.len() - 1] {
            match &item.value {
                EA::SequenceItem_::Bind(list, exp) => {
                    let (t, e) = self.translate_exp_free(exp);
                    if list.value.len() != 1 {
                        self.error(
                            &self.to_loc(&list.loc),
                            "[current restriction] tuples not supported in let",
                        );
                        return ExpData::Invalid(self.new_node_id());
                    }
                    let bind_loc = self.to_loc(&list.value[0].loc);
                    match &list.value[0].value {
                        EA::LValue_::Var(_, maccess, _) => {
                            let name = match &maccess.value {
                                EA::ModuleAccess_::Name(n) => n,
                                EA::ModuleAccess_::ModuleAccess(_, n) => n,
                                EA::ModuleAccess_::Variant(_, n) => n,
                            };
                            // Define the local. Currently we mimic
                            // Rust/ML semantics here, allowing to shadow with each let,
                            // thus entering a new scope.
                            self.enter_scope();
                            let name = self.symbol_pool().make(&name.value);
                            self.define_local(&bind_loc, name, t.clone(), None, None);
                            let id = self.new_node_id_with_type_loc(&t, &bind_loc);
                            decls.push(LocalVarDecl {
                                id,
                                name,
                                binding: Some(e.into_exp()),
                            });
                        }
                        EA::LValue_::Unpack(..) => {
                            self.error(
                                &bind_loc,
                                "[current restriction] unpack not supported in let",
                            );
                            return ExpData::Invalid(self.new_node_id());
                        }
                    }
                }
                EA::SequenceItem_::Seq(e) => {
                    let translated = self.translate_exp(e, expected_type);
                    match translated {
                        ExpData::Call(_, Operation::NoOp, _) => { /* allow assert statement */ }
                        _ => self.error(
                            &self.to_loc(&item.loc),
                            "only binding `let p = e; ...` allowed here",
                        ),
                    }
                }
                _ => self.error(
                    &self.to_loc(&item.loc),
                    "only binding `let p = e; ...` allowed here",
                ),
            }
        }

        // Process the last element, which must be an Exp item.
        let last = match &seq[n - 1].value {
            EA::SequenceItem_::Seq(e) => self.translate_exp(e, expected_type),
            _ => {
                self.error(
                    &self.to_loc(&seq[n - 1].loc),
                    "expected an expression as the last element of the block",
                );
                self.new_error_exp()
            }
        };

        // Exit the scopes for variable bindings
        for _ in 0..decls.len() {
            self.exit_scope();
        }

        let id = self.new_node_id_with_type_loc(expected_type, loc);
        ExpData::Block(id, decls, last.into_exp())
    }

    #[allow(unused)]
    pub fn make_context_local_name(&self, name: Symbol, in_old: bool) -> Symbol {
        if in_old {
            self.symbol_pool()
                .make(&format!("{}_$old", name.display(self.symbol_pool())))
        } else {
            name
        }
    }

    /// Translate an Index expression.
    fn translate_index(
        &mut self,
        loc: &Loc,
        target: &EA::Exp,
        index: &EA::Exp,
        expected_type: &Type,
    ) -> ExpData {
        // We must concretize the type of index to decide whether this is a slice
        // or not. This is not compatible with full type inference, so we may
        // try to actually represent slicing explicitly in the syntax to fix this.
        // Alternatively, we could leave it to the backend to figure (after full
        // type inference) whether this is slice or index.
        let elem_ty = self.fresh_type_var();
        let vector_ty = Type::Vector(Box::new(elem_ty.clone()));
        let vector_exp = self.translate_exp(target, &vector_ty);
        let (index_ty, ie) = self.translate_exp_free(index);
        let index_ty = self.subs.specialize(&index_ty);
        let (result_t, oper) = if let Type::Primitive(PrimitiveType::Range) = &index_ty {
            (vector_ty, Operation::Slice)
        } else {
            // If this is not (known to be) a range, assume its an index.
            self.check_type(
                loc,
                &index_ty,
                &Type::new_prim(PrimitiveType::Num),
                "in index expression",
            );
            (elem_ty, Operation::Index)
        };
        let result_t = self.check_type(loc, &result_t, expected_type, "in index expression");
        let id = self.new_node_id_with_type_loc(&result_t, loc);
        ExpData::Call(id, oper, vec![vector_exp.into_exp(), ie.into_exp()])
    }

    /// Translate a Dotted expression.
    fn translate_dotted(&mut self, dotted: &EA::ExpDotted, expected_type: &Type) -> ExpData {
        match &dotted.value {
            EA::ExpDotted_::Exp(e) => self.translate_exp(e, expected_type),
            EA::ExpDotted_::Dot(e, _, n) => {
                let loc = self.to_loc(&dotted.loc);
                let ty = self.fresh_type_var();
                let exp = self.translate_dotted(e.as_ref(), &ty);
                if let Some((struct_id, field_id, field_ty)) = self.lookup_field(&loc, &ty, n) {
                    self.check_type(&loc, &field_ty, expected_type, "in field selection");
                    let id = self.new_node_id_with_type_loc(&field_ty, &loc);
                    ExpData::Call(
                        id,
                        Operation::Select(struct_id.module_id, struct_id.id, field_id),
                        vec![exp.into_exp()],
                    )
                } else {
                    self.new_error_exp()
                }
            }
            EA::ExpDotted_::Index(_, _) => unimplemented!("translating index syntax"),
            EA::ExpDotted_::DotUnresolved(_, _) => {
                unimplemented!("translating dot unresolved syntax")
            }
        }
    }

    /// Loops up a field in a struct. Returns field information or None after reporting errors.
    fn lookup_field(
        &mut self,
        loc: &Loc,
        struct_ty: &Type,
        name: &Name,
    ) -> Option<(QualifiedId<DatatypeId>, FieldId, Type)> {
        // Similar as with Index, we must concretize the type of the expression on which
        // field selection is performed, violating pure type inference rules, so we can actually
        // check and retrieve the field. To avoid this, we would need to introduce the concept
        // of a type constraint to type unification, where the constraint would be
        // 'type var X where X has field F'. This makes unification significant more complex,
        // so lets see how far we get without this.
        let struct_ty = self.subs.specialize(struct_ty);
        let field_name = self.symbol_pool().make(&name.value);
        if let Type::Datatype(mid, sid, targs) = &struct_ty {
            // Lookup the StructEntry in the build. It must be defined for valid
            // Type::Datatype instances.
            let struct_name = self
                .parent
                .parent
                .reverse_datatype_table
                .get(&(*mid, *sid))
                .expect("invalid Type::Datatype");
            let entry = self
                .parent
                .parent
                .datatype_table
                .get(struct_name)
                .expect("invalid Type::Datatype");
            // Lookup the field in the struct.
            if let DatatypeData::Struct {
                fields: Some(fields),
            } = &entry.data
            {
                if let Some((_, field_ty)) = fields.get(&field_name) {
                    // We must instantiate the field type by the provided type args.
                    let field_ty = field_ty.instantiate(targs);
                    Some((
                        entry.module_id.qualified(entry.struct_id),
                        FieldId::new(field_name),
                        field_ty,
                    ))
                } else {
                    self.error(
                        loc,
                        &format!(
                            "field `{}` not declared in struct `{}`",
                            field_name.display(self.symbol_pool()),
                            struct_name.display(self.symbol_pool())
                        ),
                    );
                    None
                }
            } else {
                self.error(
                    loc,
                    &format!(
                        "struct `{}` is native and does not support field selection",
                        struct_name.display(self.symbol_pool())
                    ),
                );
                None
            }
        } else {
            self.error(
                loc,
                &format!(
                    "type `{}` cannot be resolved as a struct",
                    struct_ty.display(&self.type_display_context()),
                ),
            );
            None
        }
    }

    /// Creates a type instantiation based on provided actual type parameters.
    fn make_instantiation(
        &mut self,
        param_count: usize,
        context_args: Vec<Type>,
        user_args: Option<Vec<Type>>,
    ) -> (Vec<Type>, Option<String>) {
        let mut args = context_args;
        let expected_user_count = param_count - args.len();
        if let Some(types) = user_args {
            let n = types.len();
            args.extend(types);
            if n != expected_user_count {
                (
                    args,
                    Some(format!(
                        "generic count mismatch (expected {} but found {})",
                        expected_user_count, n,
                    )),
                )
            } else {
                (args, None)
            }
        } else {
            // Create fresh type variables for user args
            for _ in 0..expected_user_count {
                args.push(self.fresh_type_var());
            }
            (args, None)
        }
    }

    fn translate_pack(
        &mut self,
        loc: &Loc,
        maccess: &EA::ModuleAccess,
        generics: &Option<Vec<EA::Type>>,
        fields: &EA::Fields<EA::Exp>,
        expected_type: &Type,
    ) -> ExpData {
        let struct_name = self.parent.module_access_to_qualified(maccess);
        let struct_name_loc = self.to_loc(&maccess.loc);
        let generics = generics.as_ref().map(|ts| self.translate_types(ts));
        if let Some(entry) = self.parent.parent.datatype_table.get(&struct_name) {
            let entry = entry.clone();
            let (instantiation, diag) =
                self.make_instantiation(entry.type_params.len(), vec![], generics);
            if let Some(msg) = diag {
                self.error(loc, &msg);
                return self.new_error_exp();
            }
            if let DatatypeData::Struct {
                fields: Some(field_decls),
            } = &entry.data
            {
                let mut fields_not_covered: BTreeSet<Symbol> = BTreeSet::new();
                fields_not_covered.extend(field_decls.keys());
                let mut args = BTreeMap::new();
                for (name_loc, name_, (_, exp)) in fields.iter() {
                    let field_name = self.symbol_pool().make(name_);
                    if let Some((idx, field_ty)) = field_decls.get(&field_name) {
                        let exp = self.translate_exp(exp, &field_ty.instantiate(&instantiation));
                        fields_not_covered.remove(&field_name);
                        args.insert(idx, exp);
                    } else {
                        self.error(
                            &self.to_loc(&name_loc),
                            &format!(
                                "field `{}` not declared in struct `{}`",
                                field_name.display(self.symbol_pool()),
                                struct_name.display(self.symbol_pool())
                            ),
                        );
                    }
                }
                if !fields_not_covered.is_empty() {
                    self.error(
                        loc,
                        &format!(
                            "missing fields {}",
                            fields_not_covered
                                .iter()
                                .map(|n| format!("`{}`", n.display(self.symbol_pool())))
                                .join(", ")
                        ),
                    );
                    self.new_error_exp()
                } else {
                    let struct_ty =
                        Type::Datatype(entry.module_id, entry.struct_id, instantiation.clone());
                    let struct_ty =
                        self.check_type(loc, &struct_ty, expected_type, "in pack expression");
                    let mut args = args
                        .into_iter()
                        .sorted_by_key(|(i, _)| *i)
                        .map(|(_, e)| e.into_exp())
                        .collect_vec();
                    if args.is_empty() {
                        // The move compiler inserts a dummy field with the value of false
                        // for structs with no fields. This is also what we find in the
                        // Model metadata (i.e. a field `dummy_field`). We simulate this here
                        // for now, though it would be better to remove it everywhere as it
                        // can be confusing to users. However, its currently hard to do this,
                        // because a user could also have defined the `dummy_field`.
                        let id = self.new_node_id_with_type_loc(&BOOL_TYPE, loc);
                        args.push(ExpData::Value(id, Value::Bool(false)).into_exp());
                    }
                    let id = self.new_node_id_with_type_loc(&struct_ty, loc);
                    self.set_node_instantiation(id, instantiation);
                    ExpData::Call(id, Operation::Pack(entry.module_id, entry.struct_id), args)
                }
            } else {
                self.error(
                    &struct_name_loc,
                    &format!(
                        "native struct `{}` cannot be packed",
                        struct_name.display(self.symbol_pool())
                    ),
                );
                self.new_error_exp()
            }
        } else {
            self.error(
                &struct_name_loc,
                &format!(
                    "undeclared struct `{}`",
                    struct_name.display(self.symbol_pool())
                ),
            );
            self.new_error_exp()
        }
    }

    fn translate_lambda(
        &mut self,
        _loc: &Loc,
        _bindings: &EA::LambdaLValues,
        _body: &EA::Exp,
        _expected_type: &Type,
    ) -> ExpData {
        unimplemented!("translation of lambdas")
        /*// Enter the lambda variables into a new local scope and collect their declarations.
        self.enter_scope();
        let mut decls = vec![];
        let mut arg_types = vec![];
        for (bind, _ty) in &bindings.value {
            let loc = self.to_loc(&bind.loc);
            match &bind.value {
                EA::LValue_::Var(
                    _,
                    Spanned {
                        value: EA::ModuleAccess_::Name(n),
                        ..
                    },
                    _,
                ) => {
                    let name = self.symbol_pool().make(&n.value);
                    let ty = self.fresh_type_var();
                    let id = self.new_node_id_with_type_loc(&ty, &loc);
                    self.define_local(&loc, name, ty.clone(), None, None);
                    arg_types.push(ty);
                    decls.push(LocalVarDecl {
                        id,
                        name,
                        binding: None,
                    });
                }
                EA::LValue_::Unpack(..) | EA::LValue_::Var(..) => {
                    self.error(&loc, "[current restriction] tuples not supported in lambda")
                }
            }
        }
        // Create a fresh type variable for the body and check expected type before analyzing
        // body. This aids type inference for the lambda parameters.
        let ty = self.fresh_type_var();
        let rty = self.check_type(
            loc,
            &Type::Fun(arg_types, Box::new(ty.clone())),
            expected_type,
            "in lambda",
        );
        let rbody = self.translate_exp(body, &ty);
        self.exit_scope();
        let id = self.new_node_id_with_type_loc(&rty, loc);
        ExpData::Lambda(id, decls, rbody.into_exp())*/
    }

    fn translate_quant(
        &mut self,
        loc: &Loc,
        kind: PA::QuantKind,
        ranges: &EA::LValueWithRangeList,
        triggers: &[Vec<EA::Exp>],
        condition: &Option<Box<EA::Exp>>,
        body: &EA::Exp,
        expected_type: &Type,
    ) -> ExpData {
        let rkind = match kind.value {
            PA::QuantKind_::Forall => QuantKind::Forall,
            PA::QuantKind_::Exists => QuantKind::Exists,
            PA::QuantKind_::Choose => QuantKind::Choose,
            PA::QuantKind_::ChooseMin => QuantKind::ChooseMin,
        };

        // Enter the quantifier variables into a new local scope and collect their declarations.
        self.enter_scope();
        let mut rranges = vec![];
        for range in &ranges.value {
            // The quantified variable and its domain expression.
            let (bind, exp) = &range.value;
            let loc = self.to_loc(&bind.loc);
            let (exp_ty, rexp) = self.translate_exp_free(exp);
            let ty = self.fresh_type_var();
            let exp_ty = self.subs.specialize(&exp_ty);
            match &exp_ty {
                Type::Vector(..) => {
                    self.check_type(
                        &loc,
                        &exp_ty,
                        &Type::Vector(Box::new(ty.clone())),
                        "in quantification over vector",
                    );
                }
                Type::TypeDomain(..) => {
                    self.check_type(
                        &loc,
                        &exp_ty,
                        &Type::TypeDomain(Box::new(ty.clone())),
                        "in quantification over domain",
                    );
                }
                Type::Primitive(PrimitiveType::Range) => {
                    self.check_type(
                        &loc,
                        &ty,
                        &Type::Primitive(PrimitiveType::Num),
                        "in quantification over range",
                    );
                }
                _ => {
                    self.error(&loc, "quantified variables must range over a vector, a type domain, or a number range");
                    return self.new_error_exp();
                }
            }
            match &bind.value {
                EA::LValue_::Var(
                    _,
                    Spanned {
                        value: EA::ModuleAccess_::Name(n),
                        ..
                    },
                    _,
                ) => {
                    let name = self.symbol_pool().make(&n.value);
                    let id = self.new_node_id_with_type_loc(&ty, &loc);
                    self.define_local(&loc, name, ty.clone(), None, None);
                    let rbind = LocalVarDecl {
                        id,
                        name,
                        binding: None,
                    };
                    rranges.push((rbind, rexp.into_exp()));
                }
                EA::LValue_::Unpack(..) | EA::LValue_::Var(..) => self.error(
                    &loc,
                    "[current restriction] tuples not supported in quantifiers",
                ),
            }
        }
        let rtriggers = triggers
            .iter()
            .map(|trigger| {
                trigger
                    .iter()
                    .map(|e| self.translate_exp_free(e).1.into_exp())
                    .collect()
            })
            .collect();
        let rbody = self.translate_exp(body, &BOOL_TYPE);
        let rcondition = condition
            .as_ref()
            .map(|cond| self.translate_exp(cond, &BOOL_TYPE).into_exp());
        self.exit_scope();
        let quant_ty = if rkind.is_choice() {
            self.parent.parent.env.get_node_type(rranges[0].0.id)
        } else {
            BOOL_TYPE.clone()
        };
        self.check_type(loc, &quant_ty, expected_type, "in quantifier expression");
        let id = self.new_node_id_with_type_loc(&quant_ty, loc);
        ExpData::Quant(id, rkind, rranges, rtriggers, rcondition, rbody.into_exp())
    }

    pub fn check_type(&mut self, loc: &Loc, ty: &Type, expected: &Type, context_msg: &str) -> Type {
        // Because of Rust borrow semantics, we must temporarily detach the substitution from
        // the build. This is because we also need to inherently borrow self via the
        // type_display_context which is passed into unification.
        let mut subs = std::mem::replace(&mut self.subs, Substitution::new());
        let result = match subs.unify(Variance::Shallow, ty, expected) {
            Ok(t) => t,
            Err(err) => {
                self.error(
                    loc,
                    &format!(
                        "{} {}",
                        err.message(&self.type_display_context()),
                        context_msg
                    ),
                );
                Type::Error
            }
        };
        self.subs = subs;
        result
    }

    pub fn translate_from_move_value(&self, loc: &Loc, ty: &Type, value: &MoveValue) -> Value {
        match (ty, value) {
            (_, MoveValue::U8(n)) => Value::Number(BigInt::from_u8(*n).unwrap()),
            (_, MoveValue::U16(n)) => Value::Number(BigInt::from_u16(*n).unwrap()),
            (_, MoveValue::U32(n)) => Value::Number(BigInt::from_u32(*n).unwrap()),
            (_, MoveValue::U64(n)) => Value::Number(BigInt::from_u64(*n).unwrap()),
            (_, MoveValue::U128(n)) => Value::Number(BigInt::from_u128(*n).unwrap()),
            (_, MoveValue::U256(n)) => Value::Number(BigInt::from(n)),
            (_, MoveValue::Bool(b)) => Value::Bool(*b),
            (_, MoveValue::Address(a)) => Value::Address(crate::addr_to_big_uint(a)),
            (_, MoveValue::Signer(a)) => Value::Address(crate::addr_to_big_uint(a)),
            (Type::Vector(inner), MoveValue::Vector(vs)) => match **inner {
                Type::Primitive(PrimitiveType::U8) => {
                    let b = vs
                        .iter()
                        .filter_map(|v| match v {
                            MoveValue::U8(n) => Some(*n),
                            _ => {
                                self.error(loc, &format!("Expected u8 type, buf found: {:?}", v));
                                None
                            }
                        })
                        .collect::<Vec<u8>>();
                    Value::ByteArray(b)
                }
                Type::Primitive(PrimitiveType::Address) => {
                    let b = vs
                        .iter()
                        .filter_map(|v| match v {
                            MoveValue::Address(a) => Some(crate::addr_to_big_uint(a)),
                            _ => {
                                self.error(
                                    loc,
                                    &format!("Expected address type, but found: {:?}", v),
                                );
                                None
                            }
                        })
                        .collect::<Vec<BigUint>>();
                    Value::AddressArray(b)
                }
                _ => {
                    let b = vs
                        .iter()
                        .map(|v| self.translate_from_move_value(loc, inner, v))
                        .collect::<Vec<Value>>();
                    Value::Vector(b)
                }
            },
            (Type::Primitive(_), MoveValue::Vector(_))
            | (Type::Primitive(_), MoveValue::Struct(_))
            | (Type::Primitive(_), MoveValue::Variant(_))
            | (Type::Tuple(_), MoveValue::Vector(_))
            | (Type::Tuple(_), MoveValue::Struct(_))
            | (Type::Tuple(_), MoveValue::Variant(_))
            | (Type::Vector(_), MoveValue::Struct(_))
            | (Type::Vector(_), MoveValue::Variant(_))
            | (Type::Datatype(_, _, _), MoveValue::Vector(_))
            | (Type::Datatype(_, _, _), MoveValue::Struct(_))
            | (Type::Datatype(_, _, _), MoveValue::Variant(_))
            | (Type::TypeParameter(_), MoveValue::Vector(_))
            | (Type::TypeParameter(_), MoveValue::Struct(_))
            | (Type::TypeParameter(_), MoveValue::Variant(_))
            | (Type::Reference(_, _), MoveValue::Vector(_))
            | (Type::Reference(_, _), MoveValue::Struct(_))
            | (Type::Reference(_, _), MoveValue::Variant(_))
            | (Type::Fun(_, _), MoveValue::Vector(_))
            | (Type::Fun(_, _), MoveValue::Struct(_))
            | (Type::Fun(_, _), MoveValue::Variant(_))
            | (Type::TypeDomain(_), MoveValue::Vector(_))
            | (Type::TypeDomain(_), MoveValue::Struct(_))
            | (Type::TypeDomain(_), MoveValue::Variant(_))
            | (Type::ResourceDomain(_, _, _), MoveValue::Vector(_))
            | (Type::ResourceDomain(_, _, _), MoveValue::Struct(_))
            | (Type::ResourceDomain(_, _, _), MoveValue::Variant(_))
            | (Type::Error, MoveValue::Vector(_))
            | (Type::Error, MoveValue::Struct(_))
            | (Type::Error, MoveValue::Variant(_))
            | (Type::Var(_), MoveValue::Vector(_))
            | (Type::Var(_), MoveValue::Struct(_))
            | (Type::Var(_), MoveValue::Variant(_)) => {
                self.error(
                    loc,
                    &format!("Not yet supported constant value: {:?}", value),
                );
                Value::Bool(false)
            }
        }
    }
}
