// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Contains types and related functions.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    fmt::Formatter,
};

use move_binary_format::{file_format::TypeParameterIndex, normalized::Type as MType};
use move_core_types::language_storage::{StructTag, TypeTag};

use crate::{
    ast::QualifiedSymbol,
    model::{DatatypeId, GlobalEnv, ModuleId, QualifiedInstId, StructEnv},
    symbol::{Symbol, SymbolPool},
};

/// Represents a type.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub enum Type {
    Primitive(PrimitiveType),
    Tuple(Vec<Type>),
    Vector(Box<Type>),
    Datatype(ModuleId, DatatypeId, Vec<Type>),
    TypeParameter(u16),

    // Types only appearing in programs.
    Reference(bool, Box<Type>),

    // Types only appearing in specifications
    Fun(Vec<Type>, Box<Type>),
    TypeDomain(Box<Type>),
    ResourceDomain(ModuleId, DatatypeId, Option<Vec<Type>>),

    // Temporary types used during type checking
    Error,
    Var(u16),
}

pub const BOOL_TYPE: Type = Type::Primitive(PrimitiveType::Bool);
pub const NUM_TYPE: Type = Type::Primitive(PrimitiveType::Num);

/// Represents a primitive (builtin) type.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum PrimitiveType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    // Types only appearing in specifications
    Num,
    Range,
    EventStore,
}

/// A type substitution.
#[derive(Debug, Clone)]
pub struct Substitution {
    subs: BTreeMap<u16, Type>,
}

/// Represents an error resulting from type unification.
pub enum TypeUnificationError {
    TypeMismatch(Type, Type),
    ArityMismatch(String, usize, usize),
    CyclicSubstitution(Type, Type),
}

impl PrimitiveType {
    /// Returns true if this type is a specification language only type
    pub fn is_spec(&self) -> bool {
        use PrimitiveType::*;
        match self {
            Bool | U8 | U16 | U32 | U64 | U128 | U256 | Address | Signer => false,
            Num | Range | EventStore => true,
        }
    }

    /// Attempt to convert this type into a normalized::Type
    pub fn into_normalized_type(self) -> Option<MType> {
        use PrimitiveType::*;
        Some(match self {
            Bool => MType::Bool,
            U8 => MType::U8,
            U16 => MType::U16,
            U32 => MType::U32,
            U64 => MType::U64,
            U128 => MType::U128,
            U256 => MType::U256,
            Address => MType::Address,
            Signer => MType::Signer,
            Num | Range | EventStore => return None,
        })
    }
}

impl Type {
    pub fn new_prim(p: PrimitiveType) -> Type {
        Type::Primitive(p)
    }

    /// Determines whether this is a type parameter.
    pub fn is_type_parameter(&self) -> bool {
        matches!(self, Type::TypeParameter(..))
    }

    /// Determines whether this is a reference.
    pub fn is_reference(&self) -> bool {
        matches!(self, Type::Reference(_, _))
    }

    /// Determines whether this is a mutable reference.
    pub fn is_mutable_reference(&self) -> bool {
        matches!(self, Type::Reference(true, _))
    }

    /// Determines whether this is an immutable reference.
    pub fn is_immutable_reference(&self) -> bool {
        matches!(self, Type::Reference(false, _))
    }

    /// Determines whether this type is a struct.
    pub fn is_struct(&self) -> bool {
        matches!(self, Type::Datatype(..))
    }

    /// Determines whether this type is a vector
    pub fn is_vector(&self) -> bool {
        matches!(self, Type::Vector(..))
    }

    /// Determines whether this is a struct, or a vector of structs, or a reference to any of
    /// those.
    pub fn is_struct_or_vector_of_struct(&self) -> bool {
        match self.skip_reference() {
            Type::Datatype(..) => true,
            Type::Vector(ety) => ety.is_struct_or_vector_of_struct(),
            _ => false,
        }
    }

    /// Returns true if this type is a specification language only type or contains specification
    /// language only types
    pub fn is_spec(&self) -> bool {
        use Type::*;
        match self {
            Primitive(p) => p.is_spec(),
            Fun(..) | TypeDomain(..) | ResourceDomain(..) | Error => true,
            Var(..) | TypeParameter(..) => false,
            Tuple(ts) => ts.iter().any(|t| t.is_spec()),
            Datatype(_, _, ts) => ts.iter().any(|t| t.is_spec()),
            Vector(et) => et.is_spec(),
            Reference(_, bt) => bt.is_spec(),
        }
    }

    /// Returns true if this is a bool.
    pub fn is_bool(&self) -> bool {
        if let Type::Primitive(PrimitiveType::Bool) = self {
            return true;
        }
        false
    }

    /// Returns true if this is any number type.
    pub fn is_number(&self) -> bool {
        if let Type::Primitive(p) = self {
            if let PrimitiveType::U8
            | PrimitiveType::U16
            | PrimitiveType::U32
            | PrimitiveType::U64
            | PrimitiveType::U128
            | PrimitiveType::U256
            | PrimitiveType::Num = p
            {
                return true;
            }
        }
        false
    }
    /// Returns true if this is an address or signer type.
    pub fn is_signer_or_address(&self) -> bool {
        matches!(
            self,
            Type::Primitive(PrimitiveType::Signer) | Type::Primitive(PrimitiveType::Address)
        )
    }

    /// Return true if this is an account address
    pub fn is_address(&self) -> bool {
        matches!(self, Type::Primitive(PrimitiveType::Address))
    }

    /// Return true if this is an account address
    pub fn is_signer(&self) -> bool {
        matches!(self, Type::Primitive(PrimitiveType::Signer))
    }

    /// Test whether this type can be used to substitute a type parameter
    pub fn can_be_type_argument(&self) -> bool {
        match self {
            Type::Primitive(p) => !p.is_spec(),
            Type::Tuple(..) => false,
            Type::Vector(e) => e.can_be_type_argument(),
            Type::Datatype(_, _, insts) => insts.iter().all(|e| e.can_be_type_argument()),
            Type::TypeParameter(..) => true,
            // references cannot be a type argument
            Type::Reference(..) => false,
            // spec types cannot be a type argument
            Type::Fun(..)
            | Type::TypeDomain(..)
            | Type::ResourceDomain(..)
            | Type::Var(..)
            | Type::Error => false,
        }
    }

    /// Skip reference type.
    pub fn skip_reference(&self) -> &Type {
        if let Type::Reference(_, bt) = self {
            bt
        } else {
            self
        }
    }

    /// If this is a struct type, replace the type instantiation.
    pub fn replace_struct_instantiation(&self, inst: &[Type]) -> Type {
        match self {
            Type::Datatype(mid, sid, _) => Type::Datatype(*mid, *sid, inst.to_vec()),
            _ => self.clone(),
        }
    }

    /// If this is a struct type, return the associated struct env and type parameters.
    pub fn get_struct<'env>(
        &'env self,
        env: &'env GlobalEnv,
    ) -> Option<(StructEnv<'env>, &'env [Type])> {
        if let Type::Datatype(module_idx, struct_idx, params) = self {
            Some((env.get_module(*module_idx).into_struct(*struct_idx), params))
        } else {
            None
        }
    }

    /// If this is a struct type, return the associated QualifiedInstId.
    pub fn get_struct_id(&self, env: &GlobalEnv) -> Option<QualifiedInstId<DatatypeId>> {
        self.get_struct(env).map(|(se, inst)| {
            se.module_env
                .get_id()
                .qualified(se.get_id())
                .instantiate(inst.to_vec())
        })
    }

    /// Require this to be a struct, if so extracts its content.
    pub fn require_struct(&self) -> (ModuleId, DatatypeId, &[Type]) {
        if let Type::Datatype(mid, sid, targs) = self {
            (*mid, *sid, targs.as_slice())
        } else {
            panic!("expected `Type::Struct`, found: `{:?}`", self)
        }
    }

    /// Instantiates type parameters in this type.
    pub fn instantiate(&self, params: &[Type]) -> Type {
        if params.is_empty() {
            self.clone()
        } else {
            self.replace(Some(params), None)
        }
    }

    /// Instantiate type parameters in the vector of types.
    pub fn instantiate_vec(vec: Vec<Type>, params: &[Type]) -> Vec<Type> {
        if params.is_empty() {
            vec
        } else {
            vec.into_iter().map(|ty| ty.instantiate(params)).collect()
        }
    }

    /// Instantiate type parameters in the slice of types.
    pub fn instantiate_slice(slice: &[Type], params: &[Type]) -> Vec<Type> {
        if params.is_empty() {
            slice.to_owned()
        } else {
            slice.iter().map(|ty| ty.instantiate(params)).collect()
        }
    }

    /// Convert a partial assignment for type parameters into an instantiation.
    pub fn type_param_map_to_inst(arity: usize, map: BTreeMap<u16, Type>) -> Vec<Type> {
        let mut inst: Vec<_> = (0..arity).map(|i| Type::TypeParameter(i as u16)).collect();
        for (idx, ty) in map {
            inst[idx as usize] = ty;
        }
        inst
    }

    /// A helper function to do replacement of type parameters.
    fn replace(&self, params: Option<&[Type]>, subs: Option<&Substitution>) -> Type {
        let replace_vec = |types: &[Type]| types.iter().map(|t| t.replace(params, subs)).collect();
        match self {
            Type::TypeParameter(i) => {
                if let Some(ps) = params {
                    ps[*i as usize].clone()
                } else {
                    self.clone()
                }
            }
            Type::Var(i) => {
                if let Some(s) = subs {
                    if let Some(t) = s.subs.get(i) {
                        // Recursively call replacement again here, in case the substitution s
                        // refers to type variables.
                        // TODO: a more efficient approach is to maintain that type assignments
                        // are always fully specialized w.r.t. to the substitution.
                        t.replace(params, subs)
                    } else {
                        self.clone()
                    }
                } else {
                    self.clone()
                }
            }
            Type::Reference(is_mut, bt) => {
                Type::Reference(*is_mut, Box::new(bt.replace(params, subs)))
            }
            Type::Datatype(mid, sid, args) => Type::Datatype(*mid, *sid, replace_vec(args)),
            Type::Fun(args, result) => {
                Type::Fun(replace_vec(args), Box::new(result.replace(params, subs)))
            }
            Type::Tuple(args) => Type::Tuple(replace_vec(args)),
            Type::Vector(et) => Type::Vector(Box::new(et.replace(params, subs))),
            Type::TypeDomain(et) => Type::TypeDomain(Box::new(et.replace(params, subs))),
            Type::ResourceDomain(mid, sid, args_opt) => {
                Type::ResourceDomain(*mid, *sid, args_opt.as_ref().map(|args| replace_vec(args)))
            }
            Type::Primitive(..) | Type::Error => self.clone(),
        }
    }

    /// Checks whether this type contains a type for which the predicate is true.
    pub fn contains<P>(&self, p: &P) -> bool
    where
        P: Fn(&Type) -> bool,
    {
        if p(self) {
            true
        } else {
            let contains_vec = |ts: &[Type]| ts.iter().any(p);
            match self {
                Type::Reference(_, bt) => bt.contains(p),
                Type::Datatype(_, _, args) => contains_vec(args),
                Type::Fun(args, result) => contains_vec(args) || result.contains(p),
                Type::Tuple(args) => contains_vec(args),
                Type::Vector(et) => et.contains(p),
                _ => false,
            }
        }
    }

    /// Returns true if this type is incomplete, i.e. contains any type variables.
    pub fn is_incomplete(&self) -> bool {
        use Type::*;
        match self {
            Var(_) => true,
            Tuple(ts) => ts.iter().any(|t| t.is_incomplete()),
            Fun(ts, r) => ts.iter().any(|t| t.is_incomplete()) || r.is_incomplete(),
            Datatype(_, _, ts) => ts.iter().any(|t| t.is_incomplete()),
            Vector(et) => et.is_incomplete(),
            Reference(_, bt) => bt.is_incomplete(),
            TypeDomain(bt) => bt.is_incomplete(),
            Error | Primitive(..) | TypeParameter(_) | ResourceDomain(..) => false,
        }
    }

    /// Return true if this type contains generic types (i.e., types that can be instantiated).
    pub fn is_open(&self) -> bool {
        let mut has_var = false;
        self.visit(&mut |t| has_var = has_var || matches!(t, Type::TypeParameter(_)));
        has_var
    }

    /// Compute used modules in this type, adding them to the passed set.
    pub fn module_usage(&self, usage: &mut BTreeSet<ModuleId>) {
        use Type::*;
        match self {
            Tuple(ts) => ts.iter().for_each(|t| t.module_usage(usage)),
            Fun(ts, r) => {
                ts.iter().for_each(|t| t.module_usage(usage));
                r.module_usage(usage);
            }
            Datatype(mid, _, ts) => {
                usage.insert(*mid);
                ts.iter().for_each(|t| t.module_usage(usage));
            }
            Vector(et) => et.module_usage(usage),
            Reference(_, bt) => bt.module_usage(usage),
            TypeDomain(bt) => bt.module_usage(usage),
            _ => {}
        }
    }

    /// Attempt to convert this type into a normalized::Type
    pub fn into_struct_type(self, env: &GlobalEnv) -> Option<MType> {
        use Type::*;
        match self {
            Datatype(mid, sid, ts) => env.get_struct_type(mid, sid, &ts),
            _ => None,
        }
    }

    /// Attempt to convert this type into a normalized::Type
    pub fn into_normalized_type(self, env: &GlobalEnv) -> Option<MType> {
        use Type::*;
        match self {
            Primitive(p) => Some(p.into_normalized_type().expect("Invariant violation: unexpected spec primitive")),
            Datatype(mid, sid, ts) =>
                env.get_struct_type(mid, sid, &ts),
            Vector(et) => Some(MType::Vector(
                Box::new(et.into_normalized_type(env)
                    .expect("Invariant violation: vector type argument contains incomplete, tuple, or spec type"))
            )),
            Reference(r, t) =>
                if r {
                    Some(MType::MutableReference(Box::new(t.into_normalized_type(env).expect("Invariant violation: reference type contains incomplete, tuple, or spec type"))))
                } else {
                    Some(MType::Reference(Box::new(t.into_normalized_type(env).expect("Invariant violation: reference type contains incomplete, tuple, or spec type"))))
                }
            TypeParameter(idx) => Some(MType::TypeParameter(idx)),
            Tuple(..) | Error | Fun(..) | TypeDomain(..) | ResourceDomain(..) | Var(..) =>
                None
        }
    }

    /// Attempt to convert this type into a language_storage::StructTag
    pub fn into_struct_tag(self, env: &GlobalEnv) -> Option<StructTag> {
        self.into_struct_type(env)?.into_struct_tag()
    }

    /// Attempt to convert this type into a language_storage::TypeTag
    pub fn into_type_tag(self, env: &GlobalEnv) -> Option<TypeTag> {
        self.into_normalized_type(env)?.into_type_tag()
    }

    /// Create a `Type` from `t`
    pub fn from_type_tag(t: &TypeTag, env: &GlobalEnv) -> Self {
        use Type::*;
        match t {
            TypeTag::Bool => Primitive(PrimitiveType::Bool),
            TypeTag::U8 => Primitive(PrimitiveType::U8),
            TypeTag::U16 => Primitive(PrimitiveType::U8),
            TypeTag::U32 => Primitive(PrimitiveType::U8),
            TypeTag::U64 => Primitive(PrimitiveType::U64),
            TypeTag::U128 => Primitive(PrimitiveType::U128),
            TypeTag::U256 => Primitive(PrimitiveType::U8),
            TypeTag::Address => Primitive(PrimitiveType::Address),
            TypeTag::Signer => Primitive(PrimitiveType::Signer),
            TypeTag::Struct(s) => {
                let qid = env.find_struct_by_tag(s).unwrap_or_else(|| {
                    panic!("Invariant violation: couldn't resolve struct {:?}", s)
                });
                let type_args = s
                    .type_params
                    .iter()
                    .map(|arg| Self::from_type_tag(arg, env))
                    .collect();
                Datatype(qid.module_id, qid.id, type_args)
            }
            TypeTag::Vector(type_param) => Vector(Box::new(Self::from_type_tag(type_param, env))),
        }
    }

    /// Get the unbound type variables in the type.
    pub fn get_vars(&self) -> BTreeSet<u16> {
        let mut vars = BTreeSet::new();
        self.internal_get_vars(&mut vars);
        vars
    }

    fn internal_get_vars(&self, vars: &mut BTreeSet<u16>) {
        use Type::*;
        match self {
            Var(id) => {
                vars.insert(*id);
            }
            Tuple(ts) => ts.iter().for_each(|t| t.internal_get_vars(vars)),
            Fun(ts, r) => {
                r.internal_get_vars(vars);
                ts.iter().for_each(|t| t.internal_get_vars(vars));
            }
            Datatype(_, _, ts) => ts.iter().for_each(|t| t.internal_get_vars(vars)),
            Vector(et) => et.internal_get_vars(vars),
            Reference(_, bt) => bt.internal_get_vars(vars),
            TypeDomain(bt) => bt.internal_get_vars(vars),
            Error | Primitive(..) | TypeParameter(..) | ResourceDomain(..) => {}
        }
    }

    pub fn visit<F: FnMut(&Type)>(&self, visitor: &mut F) {
        let visit_slice = |s: &[Type], visitor: &mut F| {
            for ty in s {
                ty.visit(visitor);
            }
        };
        match self {
            Type::Tuple(tys) => visit_slice(tys, visitor),
            Type::Vector(bt) => bt.visit(visitor),
            Type::Datatype(_, _, tys) => visit_slice(tys, visitor),
            Type::Reference(_, ty) => ty.visit(visitor),
            Type::Fun(tys, ty) => {
                visit_slice(tys, visitor);
                ty.visit(visitor);
            }
            Type::TypeDomain(bt) => bt.visit(visitor),
            _ => {}
        }
        visitor(self)
    }
}

/// A parameter for type unification that specifies the type compatibility rules to follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variance {
    /// Co-variance is allowed in all depths of the recursive type unification process
    Allow,
    /// Co-variance is only allowed for the outermost type unification round
    Shallow,
    /// Co-variance is not allowed at all
    Disallow,
}

impl Substitution {
    /// Creates a new substitution.
    pub fn new() -> Self {
        Self {
            subs: BTreeMap::new(),
        }
    }

    /// Binds the type variables.
    pub fn bind(&mut self, var: u16, ty: Type) {
        self.subs.insert(var, ty);
    }

    /// Specializes the type, substituting all variables bound in this substitution.
    pub fn specialize(&self, t: &Type) -> Type {
        t.replace(None, Some(self))
    }

    /// Return either a shallow or deep substitution of the type variable.
    ///
    /// If deep substitution is requested, follow down the substitution chain until either
    /// - `Some(ty)` when the final type is not a type variable or
    /// - `None` when the final type variable does not have a substitution
    pub fn get_substitution(&self, var: u16, shallow: bool) -> Option<Type> {
        match self.subs.get(&var) {
            None => None,
            Some(Type::Var(next_var)) => {
                if shallow {
                    Some(Type::Var(*next_var))
                } else {
                    self.get_substitution(*next_var, false)
                }
            }
            Some(subst_ty) => Some(subst_ty.clone()),
        }
    }

    /// Unify two types, returning the unified type.
    ///
    /// This currently implements the following notion of type compatibility:
    ///
    /// - 1) References are dropped (i.e. &T and T are compatible)
    /// - 2) All integer types are compatible if co-variance is allowed.
    /// - 3) With the joint effect of 1) and 2), if (P, Q) is compatible under co-variance,
    ///      (&P, Q), (P, &Q), and (&P, &Q) are all compatible under co-variance.
    /// - 4) If in two tuples (P1, P2, ..., Pn) and (Q1, Q2, ..., Qn), all (Pi, Qi) pairs are
    ///      compatible under co-variance, then the two tuples are compatible under co-variance.
    ///
    /// The substitution will be refined by variable assignments as needed to perform
    /// unification. If unification fails, the substitution will be in some intermediate state;
    /// to implement transactional unification, the substitution must be cloned before calling
    /// this.
    pub fn unify(
        &mut self,
        variance: Variance,
        t1: &Type,
        t2: &Type,
    ) -> Result<Type, TypeUnificationError> {
        // Derive the variance level for recursion
        let sub_variance = match variance {
            Variance::Allow => Variance::Allow,
            Variance::Shallow | Variance::Disallow => Variance::Disallow,
        };
        // If any of the arguments is a reference, drop it for unification, but ensure
        // it is put back since we need to maintain this information for later phases.
        if let Type::Reference(is_mut, bt1) = t1 {
            // Avoid creating nested references.
            let t2 = if let Type::Reference(_, bt2) = t2 {
                bt2.as_ref()
            } else {
                t2
            };
            return Ok(Type::Reference(
                *is_mut,
                Box::new(self.unify(sub_variance, bt1.as_ref(), t2)?),
            ));
        }
        if let Type::Reference(is_mut, bt2) = t2 {
            return Ok(Type::Reference(
                *is_mut,
                Box::new(self.unify(sub_variance, t1, bt2.as_ref())?),
            ));
        }

        // Substitute or assign variables.
        if let Some(rt) = self.try_substitute_or_assign(variance, false, t1, t2)? {
            return Ok(rt);
        }
        if let Some(rt) = self.try_substitute_or_assign(variance, true, t2, t1)? {
            return Ok(rt);
        }

        // Accept any error type.
        if matches!(t1, Type::Error) {
            return Ok(t2.clone());
        }
        if matches!(t2, Type::Error) {
            return Ok(t1.clone());
        }

        // Unify matching structured types.
        match (t1, t2) {
            (Type::Primitive(p1), Type::Primitive(p2)) => {
                if p1 == p2 {
                    return Ok(t1.clone());
                }
                // All integer types are compatible if co-variance is allowed.
                if matches!(variance, Variance::Allow | Variance::Shallow)
                    && t1.is_number()
                    && t2.is_number()
                {
                    return Ok(Type::Primitive(PrimitiveType::Num));
                }
            }
            (Type::TypeParameter(idx1), Type::TypeParameter(idx2)) => {
                if idx1 == idx2 {
                    return Ok(t1.clone());
                }
            }
            (Type::Tuple(ts1), Type::Tuple(ts2)) => {
                return Ok(Type::Tuple(self.unify_vec(
                    sub_variance,
                    ts1,
                    ts2,
                    "tuples",
                )?));
            }
            (Type::Fun(ts1, r1), Type::Fun(ts2, r2)) => {
                return Ok(Type::Fun(
                    self.unify_vec(sub_variance, ts1, ts2, "functions")?,
                    Box::new(self.unify(sub_variance, r1, r2)?),
                ));
            }
            (Type::Datatype(m1, s1, ts1), Type::Datatype(m2, s2, ts2)) => {
                if m1 == m2 && s1 == s2 {
                    return Ok(Type::Datatype(
                        *m1,
                        *s1,
                        self.unify_vec(sub_variance, ts1, ts2, "structs")?,
                    ));
                }
            }
            (Type::Vector(e1), Type::Vector(e2)) => {
                return Ok(Type::Vector(Box::new(self.unify(sub_variance, e1, e2)?)));
            }
            (Type::TypeDomain(e1), Type::TypeDomain(e2)) => {
                return Ok(Type::TypeDomain(Box::new(self.unify(
                    sub_variance,
                    e1,
                    e2,
                )?)));
            }
            _ => {}
        }
        Err(TypeUnificationError::TypeMismatch(
            self.specialize(t1),
            self.specialize(t2),
        ))
    }

    /// Helper to unify two type vectors.
    fn unify_vec(
        &mut self,
        variance: Variance,
        ts1: &[Type],
        ts2: &[Type],
        item_name: &str,
    ) -> Result<Vec<Type>, TypeUnificationError> {
        if ts1.len() != ts2.len() {
            return Err(TypeUnificationError::ArityMismatch(
                item_name.to_owned(),
                ts1.len(),
                ts2.len(),
            ));
        }
        let mut rs = vec![];
        for i in 0..ts1.len() {
            rs.push(self.unify(variance, &ts1[i], &ts2[i])?);
        }
        Ok(rs)
    }

    /// Tries to substitute or assign a variable. Returned option is Some if unification
    /// was performed, None if not.
    fn try_substitute_or_assign(
        &mut self,
        variance: Variance,
        swapped: bool,
        t1: &Type,
        t2: &Type,
    ) -> Result<Option<Type>, TypeUnificationError> {
        if let Type::Var(v1) = t1 {
            if let Some(s1) = self.subs.get(v1).cloned() {
                return if swapped {
                    // Place the type terms in the right order again, so we
                    // get the 'expected vs actual' direction right.
                    Ok(Some(self.unify(variance, t2, &s1)?))
                } else {
                    Ok(Some(self.unify(variance, &s1, t2)?))
                };
            }
            let is_t1_var = |t: &Type| {
                if let Type::Var(v2) = t {
                    v1 == v2
                } else {
                    false
                }
            };
            // Skip the cycle check if we are unifying the same two variables.
            if is_t1_var(t2) {
                return Ok(Some(t1.clone()));
            }
            // Cycle check.
            if !t2.contains(&is_t1_var) {
                self.subs.insert(*v1, t2.clone());
                Ok(Some(t2.clone()))
            } else {
                // It is not clear to me whether this can ever occur given we do no global
                // unification with recursion, but to be on the save side, we have it.
                Err(TypeUnificationError::CyclicSubstitution(
                    self.specialize(t1),
                    self.specialize(t2),
                ))
            }
        } else {
            Ok(None)
        }
    }
}

impl Default for Substitution {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to unify types which stem from different generic contexts.
///
/// Both comparison side may have type parameters (equally named as #0, #1, ...).
/// The helper converts the type parameter from or both sides into variables
/// and then performs unification of the terms. The resulting substitution
/// is converted back to parameter instantiations.
///
/// Example: consider a function f<X> which uses memory M<X, u64>, and invariant
/// invariant<X> which uses memory M<bool, X>. Using this helper to unify both
/// memories will result in instantiations which when applied create f<bool>
/// and invariant<u64> respectively.
pub struct TypeUnificationAdapter {
    type_vars_map: BTreeMap<u16, (bool, TypeParameterIndex)>,
    types_adapted_lhs: Vec<Type>,
    types_adapted_rhs: Vec<Type>,
}

impl TypeUnificationAdapter {
    /// Initialize the context for the type unifier.
    ///
    /// If `treat_lhs_type_param_as_var_after_index` is set to P,
    /// - any type parameter on the LHS with index < P will be treated as concrete types and
    /// - only type parameters on the LHS with index >= P are treated as variables and thus,
    ///   participate in the type unification process.
    /// The same rule applies to the RHS parameters via `treat_rhs_type_param_as_var_after_index`.
    fn new<'a, I>(
        lhs_types: I,
        rhs_types: I,
        treat_lhs_type_param_as_var_after_index: Option<TypeParameterIndex>,
        treat_rhs_type_param_as_var_after_index: Option<TypeParameterIndex>,
    ) -> Self
    where
        I: Iterator<Item = &'a Type> + Clone,
    {
        debug_assert!(
            treat_lhs_type_param_as_var_after_index.is_some()
                || treat_rhs_type_param_as_var_after_index.is_some(),
            "At least one side of the unification must be treated as variable"
        );

        // Check the input types do not contain type variables.
        debug_assert!(
            lhs_types.clone().chain(rhs_types.clone()).all(|ty| {
                let mut b = true;
                ty.visit(&mut |t| b = b && !matches!(t, Type::Var(_)));
                b
            }),
            "unexpected type variable"
        );

        // Compute the number of type parameters for each side.
        let mut lhs_type_param_count = 0;
        let mut rhs_type_param_count = 0;
        let count_type_param = |t: &Type, current: &mut u16| {
            if let Type::TypeParameter(idx) = t {
                *current = (*current).max(*idx + 1);
            }
        };
        for ty in lhs_types.clone() {
            ty.visit(&mut |t| count_type_param(t, &mut lhs_type_param_count));
        }
        for ty in rhs_types.clone() {
            ty.visit(&mut |t| count_type_param(t, &mut rhs_type_param_count));
        }

        // Create a type variable instantiation for each side.
        let mut var_count = 0;
        let mut type_vars_map = BTreeMap::new();
        let lhs_inst = match treat_lhs_type_param_as_var_after_index {
            None => vec![],
            Some(boundary) => (0..boundary)
                .map(Type::TypeParameter)
                .chain((boundary..lhs_type_param_count).map(|i| {
                    let idx = var_count;
                    var_count += 1;
                    type_vars_map.insert(idx, (true, i));
                    Type::Var(idx)
                }))
                .collect(),
        };
        let rhs_inst = match treat_rhs_type_param_as_var_after_index {
            None => vec![],
            Some(boundary) => (0..boundary)
                .map(Type::TypeParameter)
                .chain((boundary..rhs_type_param_count).map(|i| {
                    let idx = var_count;
                    var_count += 1;
                    type_vars_map.insert(idx, (false, i));
                    Type::Var(idx)
                }))
                .collect(),
        };

        // Do the adaptation.
        let types_adapted_lhs = lhs_types.map(|t| t.instantiate(&lhs_inst)).collect();
        let types_adapted_rhs = rhs_types.map(|t| t.instantiate(&rhs_inst)).collect();

        Self {
            type_vars_map,
            types_adapted_lhs,
            types_adapted_rhs,
        }
    }

    /// Create a TypeUnificationAdapter with the goal of unifying a pair of types.
    ///
    /// If `treat_lhs_type_param_as_var` is True, treat all type parameters on the LHS as variables.
    /// If `treat_rhs_type_param_as_var` is True, treat all type parameters on the RHS as variables.
    pub fn new_pair(
        lhs_type: &Type,
        rhs_type: &Type,
        treat_lhs_type_param_as_var: bool,
        treat_rhs_type_param_as_var: bool,
    ) -> Self {
        Self::new(
            std::iter::once(lhs_type),
            std::iter::once(rhs_type),
            treat_lhs_type_param_as_var.then_some(0),
            treat_rhs_type_param_as_var.then_some(0),
        )
    }

    /// Create a TypeUnificationAdapter with the goal of unifying a pair of type tuples.
    ///
    /// If `treat_lhs_type_param_as_var` is True, treat all type parameters on the LHS as variables.
    /// If `treat_rhs_type_param_as_var` is True, treat all type parameters on the RHS as variables.
    pub fn new_vec(
        lhs_types: &[Type],
        rhs_types: &[Type],
        treat_lhs_type_param_as_var: bool,
        treat_rhs_type_param_as_var: bool,
    ) -> Self {
        Self::new(
            lhs_types.iter(),
            rhs_types.iter(),
            treat_lhs_type_param_as_var.then_some(0),
            treat_rhs_type_param_as_var.then_some(0),
        )
    }

    /// Consume the TypeUnificationAdapter and produce the unification result. If type unification
    /// is successful, return a pair of instantiations for type parameters on each side which
    /// unify the LHS and RHS respectively. If the LHS and RHS cannot unify, None is returned.
    pub fn unify(
        self,
        variance: Variance,
        shallow_subst: bool,
    ) -> Option<(BTreeMap<u16, Type>, BTreeMap<u16, Type>)> {
        let mut subst = Substitution::new();
        match subst.unify_vec(
            variance,
            &self.types_adapted_lhs,
            &self.types_adapted_rhs,
            "",
        ) {
            Ok(_) => {
                let mut inst_lhs = BTreeMap::new();
                let mut inst_rhs = BTreeMap::new();
                for (var_idx, (is_lhs, param_idx)) in &self.type_vars_map {
                    let subst_ty = match subst.get_substitution(*var_idx, shallow_subst) {
                        None => continue,
                        Some(Type::Var(subst_var_idx)) => {
                            match self.type_vars_map.get(&subst_var_idx) {
                                None => {
                                    // If the original types do not contain free type
                                    // variables, this should not happen.
                                    panic!("unexpected type variable");
                                }
                                Some((_, subs_param_idx)) => {
                                    // There can be either lhs or rhs type parameters left, but
                                    // not both sides, so it is unambiguous to just return it here.
                                    Type::TypeParameter(*subs_param_idx)
                                }
                            }
                        }
                        Some(subst_ty) => subst_ty.clone(),
                    };
                    let inst = if *is_lhs {
                        &mut inst_lhs
                    } else {
                        &mut inst_rhs
                    };
                    inst.insert(*param_idx, subst_ty);
                }

                Some((inst_lhs, inst_rhs))
            }
            Err(_) => None,
        }
    }
}

impl TypeUnificationError {
    pub fn message(&self, display_context: &TypeDisplayContext) -> String {
        match self {
            TypeUnificationError::TypeMismatch(t1, t2) => {
                format!(
                    "expected `{}` but found `{}`",
                    t2.display(display_context),
                    t1.display(display_context),
                )
            }
            TypeUnificationError::ArityMismatch(item, a1, a2) => {
                format!("{} have different arity ({} != {})", item, a1, a2)
            }
            TypeUnificationError::CyclicSubstitution(t1, t2) => {
                format!(
                    "[internal] type unification cycle check failed ({} =?= {})",
                    t1.display(display_context),
                    t2.display(display_context),
                )
            }
        }
    }
}

/// A helper to derive the set of instantiations for type parameters
pub struct TypeInstantiationDerivation {}

impl TypeInstantiationDerivation {
    /// Find what the instantiations should we have for the type parameter at `target_param_index`.
    ///
    /// The invariant is, forall type parameters whose index < target_param_index, it should either
    /// - be assigned with a concrete type already and hence, ceases to be a type parameter, or
    /// - does not have any matching instantiation and hence, either remains a type parameter or is
    ///   represented as a type error.
    /// But in anyway, these type parameters no longer participate in type unification anymore.
    ///
    /// If `target_lhs` is True, derive instantiations for the type parameter with
    /// `target_param_index` on the `lhs_types`. Otherwise, target the `rhs_types`.
    fn derive_instantiations_for_target_parameter(
        lhs_types: &BTreeSet<Type>,
        rhs_types: &BTreeSet<Type>,
        treat_lhs_type_param_as_var: bool,
        treat_rhs_type_param_as_var: bool,
        target_param_index: TypeParameterIndex,
        target_lhs: bool,
    ) -> BTreeSet<Type> {
        // progressively increase the boundary
        let treat_lhs_type_param_as_var_after_index =
            treat_lhs_type_param_as_var.then_some(if target_lhs { target_param_index } else { 0 });
        let treat_rhs_type_param_as_var_after_index =
            treat_rhs_type_param_as_var.then_some(if target_lhs { 0 } else { target_param_index });

        let mut target_param_insts = BTreeSet::new();
        for t_lhs in lhs_types {
            for t_rhs in rhs_types {
                // Try to unify the instantiations
                let adapter = TypeUnificationAdapter::new(
                    std::iter::once(t_lhs),
                    std::iter::once(t_rhs),
                    treat_lhs_type_param_as_var_after_index,
                    treat_rhs_type_param_as_var_after_index,
                );
                let rel = adapter.unify(Variance::Allow, false);
                if let Some((subst_lhs, subst_rhs)) = rel {
                    let subst = if target_lhs { subst_lhs } else { subst_rhs };
                    for (param_idx, inst_ty) in subst.into_iter() {
                        if param_idx != target_param_index {
                            // this parameter will be unified at a later stage.
                            //
                            // NOTE: this code is inefficient when we have multiple type parameters,
                            // but a vast majority of Move code we see so far have at most one type
                            // parameter, so we trade-off efficiency with simplicity in code.
                            assert!(param_idx > target_param_index);
                            continue;
                        }
                        target_param_insts.insert(inst_ty);
                    }
                }
            }
        }
        target_param_insts
    }

    /// Find the set of valid instantiation combinations for all the type parameters.
    ///
    /// The algorithm is progressive. For a list of parameters with arity `params_arity = N`, it
    /// - first finds all possible instantiation for parameter at index 0 (`inst_param_0`) and,'
    /// - for each instantiation in `inst_param_0`,
    ///   - refines LHS or RHS types and
    ///   - finds all possible instantiations for parameter at index 1 (`inst_param_1`)
    ///   - for each instantiation in `inst_param_1`,
    ///     - refines LHS or RHS types and
    ///     - finds all possible instantiations for parameter at index 2 (`inst_param_2`)
    ///     - for each instantiation in `inst_param_2`,
    ///       - ......
    /// The process continues until all type parameters are analyzed (i.e., reaching the type
    /// parameter at index `N`).
    ///
    /// If `refine_lhs` is True, refine the `lhs_types` after each round; same for `refine_rhs`.
    ///
    /// If `target_lhs` is True, find instantiations for the type parameters in the `lhs_types`,
    /// otherwise, target the `rhs_types`.
    ///
    /// If `mark_irrelevant_param_as_error` is True, type parameters that do not have any valid
    /// instantiation will be marked as `Type::Error`. Otherwise, leave the type parameter as it is.
    pub fn progressive_instantiation<'a, I>(
        lhs_types: I,
        rhs_types: I,
        treat_lhs_type_param_as_var: bool,
        treat_rhs_type_param_as_var: bool,
        refine_lhs: bool,
        refine_rhs: bool,
        params_arity: usize,
        target_lhs: bool,
        mark_irrelevant_param_as_error: bool,
    ) -> BTreeSet<Vec<Type>>
    where
        I: Iterator<Item = &'a Type> + Clone,
    {
        let initial_param_insts: Vec<_> = (0..params_arity)
            .map(|idx| Type::TypeParameter(idx as TypeParameterIndex))
            .collect();

        let mut work_queue = VecDeque::new();
        work_queue.push_back(initial_param_insts);
        for target_param_index in 0..params_arity {
            let mut for_next_round = vec![];
            while let Some(param_insts) = work_queue.pop_front() {
                // refine the memory usage sets with current param instantiations
                let refined_lhs = lhs_types
                    .clone()
                    .map(|t| {
                        if refine_lhs {
                            t.instantiate(&param_insts)
                        } else {
                            t.clone()
                        }
                    })
                    .collect();
                let refined_rhs = rhs_types
                    .clone()
                    .map(|t| {
                        if refine_rhs {
                            t.instantiate(&param_insts)
                        } else {
                            t.clone()
                        }
                    })
                    .collect();

                // find type instantiations for the target parameter index
                let mut target_param_insts = Self::derive_instantiations_for_target_parameter(
                    &refined_lhs,
                    &refined_rhs,
                    treat_lhs_type_param_as_var,
                    treat_rhs_type_param_as_var,
                    target_param_index as TypeParameterIndex,
                    target_lhs,
                );

                // decide what to do with an irrelevant type parameter
                if target_param_insts.is_empty() {
                    let irrelevant_type = if mark_irrelevant_param_as_error {
                        Type::Error
                    } else {
                        Type::TypeParameter(target_param_index as TypeParameterIndex)
                    };
                    target_param_insts.insert(irrelevant_type);
                }

                // instantiate the target type parameter in every possible way
                for inst in target_param_insts {
                    let mut next_insts = param_insts.clone();
                    next_insts[target_param_index] = inst;
                    for_next_round.push(next_insts);
                }
            }
            work_queue.extend(for_next_round);
        }

        // the final workqueue contains possible instantiations for all type parameters
        work_queue.into_iter().collect()
    }
}

/// Data providing context for displaying types.
pub enum TypeDisplayContext<'a> {
    WithoutEnv {
        symbol_pool: &'a SymbolPool,
        reverse_struct_table: &'a BTreeMap<(ModuleId, DatatypeId), QualifiedSymbol>,
    },
    WithEnv {
        env: &'a GlobalEnv,
        type_param_names: Option<Vec<Symbol>>,
    },
}

impl<'a> TypeDisplayContext<'a> {
    pub fn symbol_pool(&self) -> &SymbolPool {
        match self {
            TypeDisplayContext::WithEnv { env, .. } => env.symbol_pool(),
            TypeDisplayContext::WithoutEnv { symbol_pool, .. } => symbol_pool,
        }
    }
}

/// Helper for type displays.
pub struct TypeDisplay<'a> {
    type_: &'a Type,
    context: &'a TypeDisplayContext<'a>,
}

impl Type {
    pub fn display<'a>(&'a self, context: &'a TypeDisplayContext<'a>) -> TypeDisplay<'a> {
        TypeDisplay {
            type_: self,
            context,
        }
    }
}

impl<'a> fmt::Display for TypeDisplay<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use Type::*;
        let comma_list = |f: &mut Formatter<'_>, ts: &[Type]| -> fmt::Result {
            let mut first = true;
            for t in ts {
                if first {
                    first = false
                } else {
                    f.write_str(", ")?;
                }
                write!(f, "{}", t.display(self.context))?;
            }
            Ok(())
        };
        match self.type_ {
            Primitive(p) => write!(f, "{}", p),
            Tuple(ts) => {
                f.write_str("(")?;
                comma_list(f, ts)?;
                f.write_str(")")
            }
            Vector(t) => write!(f, "vector<{}>", t.display(self.context)),
            TypeDomain(t) => write!(f, "domain<{}>", t.display(self.context)),
            ResourceDomain(mid, sid, inst_opt) => {
                write!(f, "resources<{}", self.struct_str(*mid, *sid))?;
                if let Some(inst) = inst_opt {
                    f.write_str("<")?;
                    comma_list(f, inst)?;
                    f.write_str(">")?;
                }
                f.write_str(">")
            }
            Fun(ts, t) => {
                f.write_str("|")?;
                comma_list(f, ts)?;
                f.write_str("|")?;
                write!(f, "{}", t.display(self.context))
            }
            Datatype(mid, sid, ts) => {
                write!(f, "{}", self.struct_str(*mid, *sid))?;
                if !ts.is_empty() {
                    f.write_str("<")?;
                    comma_list(f, ts)?;
                    f.write_str(">")?;
                }
                Ok(())
            }
            Reference(is_mut, t) => {
                f.write_str("&")?;
                if *is_mut {
                    f.write_str("mut ")?;
                }
                write!(f, "{}", t.display(self.context))
            }
            TypeParameter(idx) => {
                if let TypeDisplayContext::WithEnv {
                    env,
                    type_param_names: Some(names),
                } = self.context
                {
                    let idx = *idx as usize;
                    if idx < names.len() {
                        write!(f, "{}", names[idx].display(env.symbol_pool()))
                    } else {
                        write!(f, "#{}", idx)
                    }
                } else {
                    write!(f, "#{}", idx)
                }
            }
            Var(idx) => write!(f, "?{}", idx),
            Error => f.write_str("*error*"),
        }
    }
}

impl<'a> TypeDisplay<'a> {
    fn struct_str(&self, mid: ModuleId, sid: DatatypeId) -> String {
        match self.context {
            TypeDisplayContext::WithoutEnv {
                symbol_pool,
                reverse_struct_table,
            } => {
                if let Some(sym) = reverse_struct_table.get(&(mid, sid)) {
                    sym.display(symbol_pool).to_string()
                } else {
                    "??unknown??".to_string()
                }
            }
            TypeDisplayContext::WithEnv { env, .. } => {
                let struct_env = env.get_module(mid).into_struct(sid);
                format!(
                    "{}::{}",
                    struct_env.module_env.get_name().display(env.symbol_pool()),
                    struct_env.get_name().display(env.symbol_pool())
                )
            }
        }
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use PrimitiveType::*;
        match self {
            Bool => f.write_str("bool"),
            U8 => f.write_str("u8"),
            U16 => f.write_str("u16"),
            U32 => f.write_str("u32"),
            U64 => f.write_str("u64"),
            U128 => f.write_str("u128"),
            U256 => f.write_str("u256"),
            Address => f.write_str("address"),
            Signer => f.write_str("signer"),
            Range => f.write_str("range"),
            Num => f.write_str("num"),
            EventStore => f.write_str("estore"),
        }
    }
}
