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
    ast::Value,
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
        temp_index: Option<usize>,
    ) {
        self.internal_define_local(loc, name, type_, temp_index)
    }

    /// Defines a let local.
    pub fn define_let_local(&mut self, loc: &Loc, name: Symbol, type_: Type) {
        self.internal_define_local(loc, name, type_, None)
    }

    fn internal_define_local(
        &mut self,
        loc: &Loc,
        name: Symbol,
        type_: Type,
        temp_index: Option<usize>,
    ) {
        let entry = LocalVarEntry {
            loc: loc.clone(),
            type_,
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

    #[allow(unused)]
    pub fn make_context_local_name(&self, name: Symbol, in_old: bool) -> Symbol {
        if in_old {
            self.symbol_pool()
                .make(&format!("{}_$old", name.display(self.symbol_pool())))
        } else {
            name
        }
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
